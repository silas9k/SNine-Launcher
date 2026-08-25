use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use serde::Deserialize;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

static RPC_ENABLED: AtomicBool = AtomicBool::new(true);

#[derive(Debug, Clone, Deserialize)]
struct DiscordRpcConfig {
    enabled: bool,
    client_id: String,
    details: String,
    state: String,
    large_image: String,
    large_text: String,
    reconnect_seconds: u64,
}

fn load_config() -> Option<DiscordRpcConfig> {
    match serde_json::from_str::<DiscordRpcConfig>(include_str!("../discord-rpc.json")) {
        Ok(config) => Some(config),
        Err(error) => {
            eprintln!("[discord-rpc] Konfiguration konnte nicht gelesen werden: {error}");
            None
        }
    }
}

fn create_activity(config: &DiscordRpcConfig) -> activity::Activity<'_> {
    activity::Activity::new()
        .details(&config.details)
        .state(&config.state)
        .assets(
            activity::Assets::new()
                .large_image(&config.large_image)
                .large_text(&config.large_text),
        )
}

pub fn start() {
    let Some(config) = load_config() else {
        return;
    };

    RPC_ENABLED.store(config.enabled, Ordering::Relaxed);

    if config.client_id.len() < 17 || !config.client_id.chars().all(|character| character.is_ascii_digit()) {
        eprintln!(
            "[discord-rpc] Deaktiviert oder keine gültige Discord Application ID eingetragen."
        );
        return;
    }

    let _ = thread::Builder::new()
        .name("s9lab-discord-rpc".to_string())
        .spawn(move || {
            let retry_delay = Duration::from_secs(config.reconnect_seconds.clamp(5, 120));

            loop {
                if !RPC_ENABLED.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(500));
                    continue;
                }

                let mut client = DiscordIpcClient::new(&config.client_id);

                match client.connect() {
                    Ok(()) => {
                        eprintln!("[discord-rpc] Mit Discord verbunden.");

                        if let Err(error) = client.set_activity(create_activity(&config)) {
                            eprintln!(
                                "[discord-rpc] Aktivität konnte nicht gesetzt werden: {error}"
                            );
                        }

                        'connected: loop {
                            for _ in 0..30 {
                                thread::sleep(Duration::from_secs(1));
                                if !RPC_ENABLED.load(Ordering::Relaxed) {
                                    let _ = client.clear_activity();
                                    let _ = client.close();
                                    eprintln!("[discord-rpc] Vom Benutzer deaktiviert.");
                                    break 'connected;
                                }
                            }

                            if let Err(error) = client.set_activity(create_activity(&config)) {
                                eprintln!(
                                    "[discord-rpc] Verbindung verloren, neuer Versuch folgt: {error}"
                                );
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        eprintln!(
                            "[discord-rpc] Discord ist nicht erreichbar, neuer Versuch folgt: {error}"
                        );
                    }
                }

                thread::sleep(retry_delay);
            }
        });
}

#[tauri::command]
pub fn discord_rpc_set_enabled(enabled: bool) {
    RPC_ENABLED.store(enabled, Ordering::Relaxed);
    eprintln!(
        "[discord-rpc] Einstellung geändert: {}",
        if enabled { "aktiv" } else { "inaktiv" }
    );
}
