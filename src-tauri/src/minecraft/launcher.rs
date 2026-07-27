use crate::{
    app::{config, paths, state::AppState},
    auth::{
        microsoft,
        model::{Account, AccountSession},
        service::AuthService,
    },
    error::{AppError, AppResult},
    logging,
    minecraft::{
        installer, java,
        manifest::{self, FeatureSet},
    },
};
use chrono::Utc;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Stdio,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    time::sleep,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LaunchState {
    Idle,
    Starting,
    Running,
    Stopping,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchStatus {
    pub state: LaunchState,
    pub process_id: Option<u32>,
    pub account_name: Option<String>,
    pub started_at_unix: Option<i64>,
    pub message: Option<String>,
    pub running_instances: usize,
}

impl LaunchStatus {
    pub fn idle() -> Self {
        Self {
            state: LaunchState::Idle,
            process_id: None,
            account_name: None,
            started_at_unix: None,
            message: None,
            running_instances: 0,
        }
    }
}

pub struct RunningInstance {
    pub child: Child,
    pub account_name: String,
    pub started_at_unix: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEvent {
    pub stream: String,
    pub line: String,
    pub timestamp_unix: i64,
}

pub async fn get_status(state: &AppState) -> LaunchStatus {
    state.launch.read().await.clone()
}

pub async fn launch_client(
    app: AppHandle,
    state: AppState,
    auth: &AuthService,
    account_id: &str,
) -> AppResult<LaunchStatus> {
    cleanup_finished_children(&state).await?;
    let running_instances = state.children.lock().await.len();

    update_status(
        &app,
        &state,
        LaunchStatus {
            state: LaunchState::Starting,
            process_id: None,
            account_name: None,
            started_at_unix: None,
            message: Some("Account und Installation werden geprüft".into()),
            running_instances,
        },
    )
    .await;

    let result = prepare_and_spawn(&app, &state, auth, account_id).await;
    if let Err(error) = &result {
        let message = error.to_string();
        logging::append(&format!("Start fehlgeschlagen: {message}"))?;
        update_status(
            &app,
            &state,
            LaunchStatus {
                state: LaunchState::Failed,
                process_id: None,
                account_name: None,
                started_at_unix: None,
                message: Some(message),
                running_instances: state.children.lock().await.len(),
            },
        )
        .await;
    }
    result
}

async fn prepare_and_spawn(
    app: &AppHandle,
    state: &AppState,
    auth: &AuthService,
    account_id: &str,
) -> AppResult<LaunchStatus> {
    let settings = config::load_settings()?;
    let game = paths::game_paths(&settings.game_directory)?;
    let record = installer::load_install_record()?;
    if record.game_version != settings.game_version {
        return Err(AppError::ClientNotInstalled);
    }
    let version = installer::load_version_json(&game, &record.game_version)?;
    let fabric = installer::load_fabric_profile(&game, &record.fabric_profile_id)?;
    installer::sync_bundled_mods(&game)?;
    let mod_client = reqwest::Client::builder()
        .user_agent(concat!("S9Lab-Launcher/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(180))
        .build()?;
    installer::sync_wavey_capes(&mod_client, &game).await?;
    let runtime = java::ensure_java(settings.java_path.as_deref()).await?;
    let required_java = version
        .java_version
        .as_ref()
        .map(|java| java.major_version)
        .unwrap_or(21);
    if runtime.major_version < required_java {
        return Err(AppError::Message(format!(
            "Minecraft benötigt Java {required_java}, gefunden wurde Java {}.",
            runtime.major_version
        )));
    }
    let (account, session) = auth.ensure_minecraft_session(account_id).await?;
    let args = build_arguments(
        &game,
        &record,
        &version,
        &fabric,
        &account,
        &session,
        settings.memory_mb,
    )?;

    logging::append(&format!(
        "Minecraft Start: {} mit Java {}",
        account.username, runtime.path
    ))?;
    emit_log(
        app,
        "system",
        &format!("Starte S9Lab Client für {}", account.username),
    );
    let mut command = Command::new(&runtime.path);
    command
        .args(&args)
        .current_dir(&game.root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("MINECRAFT_LAUNCHER_BRAND", "S9LabLauncher")
        .env("MINECRAFT_LAUNCHER_VERSION", env!("CARGO_PKG_VERSION"));
    configure_windows_process(&mut command);
    let mut child = command.spawn()?;
    let process_id = child.id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let started_at_unix = Utc::now().timestamp();
    state.children.lock().await.push(RunningInstance {
        child,
        account_name: account.username.clone(),
        started_at_unix,
    });
    let running_instances = state.children.lock().await.len();

    let status = LaunchStatus {
        state: LaunchState::Running,
        process_id,
        account_name: Some(account.username.clone()),
        started_at_unix: Some(started_at_unix),
        message: Some(format!(
            "S9Lab Client läuft · {running_instances} Instanz(en)"
        )),
        running_instances,
    };
    update_status(app, state, status.clone()).await;

    if let Some(stdout) = stdout {
        spawn_log_reader(app.clone(), stdout, "stdout");
    }
    if let Some(stderr) = stderr {
        spawn_log_reader(app.clone(), stderr, "stderr");
    }
    spawn_process_monitor(app.clone(), state.clone(), process_id);

    if settings.close_on_launch {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
    }
    Ok(status)
}

pub async fn stop_client(app: &AppHandle, state: &AppState) -> AppResult<LaunchStatus> {
    let current = state.launch.read().await.clone();
    update_status(
        app,
        state,
        LaunchStatus {
            state: LaunchState::Stopping,
            process_id: current.process_id,
            account_name: current.account_name,
            started_at_unix: current.started_at_unix,
            message: Some("Minecraft-Instanzen werden beendet".into()),
            running_instances: current.running_instances,
        },
    )
    .await;

    let mut guard = state.children.lock().await;
    for instance in guard.iter_mut() {
        instance.child.kill().await?;
        let _ = instance.child.wait().await;
    }
    guard.clear();
    drop(guard);

    let status = LaunchStatus::idle();
    update_status(app, state, status.clone()).await;
    logging::append("Alle Minecraft-Instanzen wurden über den Launcher beendet")?;
    Ok(status)
}

fn build_arguments(
    game: &paths::GamePaths,
    record: &installer::InstallRecord,
    version: &manifest::VersionJson,
    fabric: &manifest::FabricProfile,
    account: &Account,
    session: &AccountSession,
    memory_mb: u32,
) -> AppResult<Vec<String>> {
    let features = FeatureSet::launch_defaults();
    let classpath = build_classpath(game, version, fabric, &features)?;
    let separator = if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    };
    let classpath_string = classpath
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(separator);
    let native_dir = game.natives.join(&record.game_version);
    let asset_index = version
        .assets
        .clone()
        .unwrap_or_else(|| version.asset_index.id.clone());
    let mut replacements = HashMap::new();
    replacements.insert(
        "${natives_directory}",
        native_dir.to_string_lossy().to_string(),
    );
    replacements.insert("${launcher_name}", "S9Lab Launcher".to_string());
    replacements.insert("${launcher_version}", env!("CARGO_PKG_VERSION").to_string());
    replacements.insert("${classpath}", classpath_string);
    replacements.insert("${classpath_separator}", separator.to_string());
    replacements.insert(
        "${library_directory}",
        game.libraries.to_string_lossy().to_string(),
    );
    replacements.insert("${auth_player_name}", account.username.clone());
    replacements.insert("${version_name}", fabric.id.clone());
    replacements.insert("${game_directory}", game.root.to_string_lossy().to_string());
    replacements.insert("${assets_root}", game.assets.to_string_lossy().to_string());
    replacements.insert("${assets_index_name}", asset_index);
    replacements.insert("${auth_uuid}", account.id.clone());
    replacements.insert(
        "${auth_access_token}",
        session
            .minecraft_access_token
            .clone()
            .ok_or_else(|| AppError::coded("auth_minecraft_session_missing"))?,
    );
    replacements.insert("${clientid}", microsoft::MICROSOFT_CLIENT_ID.to_string());
    replacements.insert("${auth_xuid}", session.xuid.clone().unwrap_or_default());
    replacements.insert("${user_type}", "msa".to_string());
    replacements.insert("${version_type}", "release".to_string());
    replacements.insert("${user_properties}", "{}".to_string());
    replacements.insert("${resolution_width}", "1280".to_string());
    replacements.insert("${resolution_height}", "720".to_string());

    let mut jvm = vec![
        format!("-Xms{}M", (memory_mb / 2).max(1024)),
        format!("-Xmx{memory_mb}M"),
    ];
    if let Some(arguments) = &version.arguments {
        jvm.extend(manifest::flatten_arguments(&arguments.jvm, &features));
    } else {
        jvm.extend([
            format!("-Djava.library.path={}", native_dir.to_string_lossy()),
            "-cp".into(),
            "${classpath}".into(),
        ]);
    }
    if let Some(arguments) = &fabric.arguments {
        jvm.extend(manifest::flatten_arguments(&arguments.jvm, &features));
    }
    if let (Some(logging), Some(path)) = (
        &version.logging,
        installer::logging_config_path(game, version),
    ) {
        jvm.push(
            logging
                .client
                .argument
                .replace("${path}", &path.to_string_lossy()),
        );
    }
    jvm.push(fabric.main_class.clone());

    let mut game_args = if let Some(arguments) = &version.arguments {
        manifest::flatten_arguments(&arguments.game, &features)
    } else {
        version
            .minecraft_arguments
            .as_deref()
            .unwrap_or_default()
            .split_whitespace()
            .map(ToOwned::to_owned)
            .collect()
    };
    if let Some(arguments) = &fabric.arguments {
        game_args.extend(manifest::flatten_arguments(&arguments.game, &features));
    }
    jvm.extend(game_args);
    Ok(jvm
        .into_iter()
        .map(|argument| replace_variables(&argument, &replacements))
        .collect())
}

fn build_classpath(
    game: &paths::GamePaths,
    version: &manifest::VersionJson,
    fabric: &manifest::FabricProfile,
    features: &FeatureSet,
) -> AppResult<Vec<PathBuf>> {
    let mut paths_out = Vec::new();
    let mut seen = HashSet::new();
    for library in version
        .libraries
        .iter()
        .filter(|library| manifest::rules_allow(&library.rules, features))
    {
        if let Some(path) = installer::library_artifact_path(game, library)? {
            push_unique(&mut paths_out, &mut seen, path);
        }
    }
    for library in &fabric.libraries {
        push_unique(
            &mut paths_out,
            &mut seen,
            installer::fabric_library_path(game, library)?,
        );
    }
    push_unique(
        &mut paths_out,
        &mut seen,
        installer::version_jar_path(game, &version.id),
    );
    for path in &paths_out {
        if !path.exists() {
            return Err(AppError::Message(format!(
                "Installationsdatei fehlt: {}. Bitte den Client reparieren.",
                path.display()
            )));
        }
    }
    Ok(paths_out)
}

fn push_unique(output: &mut Vec<PathBuf>, seen: &mut HashSet<String>, path: PathBuf) {
    let key = path.to_string_lossy().to_lowercase();
    if seen.insert(key) {
        output.push(path);
    }
}

fn replace_variables(value: &str, replacements: &HashMap<&str, String>) -> String {
    replacements
        .iter()
        .fold(value.to_string(), |current, (key, replacement)| {
            current.replace(key, replacement)
        })
}

fn spawn_log_reader<R>(app: AppHandle, reader: R, stream: &'static str)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            emit_log(&app, stream, &line);
        }
    });
}

fn spawn_process_monitor(app: AppHandle, state: AppState, process_id: Option<u32>) {
    tauri::async_runtime::spawn(async move {
        let Some(process_id) = process_id else {
            return;
        };
        loop {
            sleep(Duration::from_secs(1)).await;
            let exit = {
                let mut guard = state.children.lock().await;
                let Some(index) = guard
                    .iter()
                    .position(|instance| instance.child.id() == Some(process_id))
                else {
                    return;
                };
                match guard[index].child.try_wait() {
                    Ok(Some(status)) => {
                        guard.remove(index);
                        Some(Ok(status))
                    }
                    Ok(None) => None,
                    Err(error) => {
                        guard.remove(index);
                        Some(Err(error))
                    }
                }
            };
            let Some(exit) = exit else {
                continue;
            };
            let message = match exit {
                Ok(status) if status.success() => {
                    format!("Minecraft-Instanz wurde beendet ({status})")
                }
                Ok(status) => format!("Minecraft-Instanz wurde mit {status} beendet"),
                Err(error) => format!("Minecraft-Prozessfehler: {error}"),
            };

            let status = {
                let guard = state.children.lock().await;
                if let Some(instance) = guard.last() {
                    LaunchStatus {
                        state: LaunchState::Running,
                        process_id: instance.child.id(),
                        account_name: Some(instance.account_name.clone()),
                        started_at_unix: Some(instance.started_at_unix),
                        message: Some(format!("S9Lab Client läuft · {} Instanz(en)", guard.len())),
                        running_instances: guard.len(),
                    }
                } else {
                    let mut idle = LaunchStatus::idle();
                    idle.message = Some(message.clone());
                    idle
                }
            };
            update_status(&app, &state, status).await;
            emit_log(&app, "system", &message);
            let _ = logging::append(&message);
            if state.children.lock().await.is_empty() {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                }
            }
            return;
        }
    });
}

async fn cleanup_finished_children(state: &AppState) -> AppResult<()> {
    let mut guard = state.children.lock().await;
    let mut index = 0;
    while index < guard.len() {
        if guard[index].child.try_wait()?.is_some() {
            guard.remove(index);
        } else {
            index += 1;
        }
    }
    Ok(())
}

async fn update_status(app: &AppHandle, state: &AppState, status: LaunchStatus) {
    *state.launch.write().await = status.clone();
    let _ = app.emit("launch-status", status);
}

fn emit_log(app: &AppHandle, stream: &str, line: &str) {
    let _ = app.emit(
        "launch-log",
        LogEvent {
            stream: stream.to_string(),
            line: line.to_string(),
            timestamp_unix: Utc::now().timestamp(),
        },
    );
}

#[cfg(target_os = "windows")]
fn configure_windows_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.as_std_mut().creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn configure_windows_process(_command: &mut Command) {}
