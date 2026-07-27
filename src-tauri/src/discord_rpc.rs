use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use serde::Deserialize;
use std::{thread, time::Duration};

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

impl DiscordRpcConfig {
    fn is_ready(&self) -> bool {
        self.enabled
            && self.client_id.len() >= 17
            && self
                .client_id
                .chars()
                .all(|character| character.is_ascii_digit())
    }
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

    if !config.is_ready() {
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
                let mut client = DiscordIpcClient::new(&config.client_id);

                match client.connect() {
                    Ok(()) => {
                        eprintln!("[discord-rpc] Mit Discord verbunden.");

                        if let Err(error) = client.set_activity(create_activity(&config)) {
                            eprintln!(
                                "[discord-rpc] Aktivität konnte nicht gesetzt werden: {error}"
                            );
                        }

                        loop {
                            thread::sleep(Duration::from_secs(30));

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
