use base64::Engine as _;
use std::io::{Cursor, Read};

const MINECRAFT_FONT_ARCHIVE_URL: &str = "https://dl.dafont.com/dl/?f=minecraft";
const MAX_FONT_ARCHIVE_BYTES: usize = 2 * 1024 * 1024;
const MAX_FONT_BYTES: usize = 512 * 1024;

#[tauri::command]
pub async fn launcher_minecraft_font_data_url() -> Result<String, String> {
    let response = reqwest::Client::builder()
        .user_agent("SNineLauncher/1.0")
        .build()
        .map_err(|error| format!("minecraft_font_client_failed:{error}"))?
        .get(MINECRAFT_FONT_ARCHIVE_URL)
        .send()
        .await
        .map_err(|error| format!("minecraft_font_download_failed:{error}"))?;

    if !response.status().is_success() {
        return Err(format!("minecraft_font_http_status:{}", response.status()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("minecraft_font_body_failed:{error}"))?;
    if bytes.is_empty() || bytes.len() > MAX_FONT_ARCHIVE_BYTES {
        return Err("minecraft_font_archive_size_invalid".into());
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.as_ref()))
        .map_err(|error| format!("minecraft_font_archive_invalid:{error}"))?;
    let mut font = None;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("minecraft_font_archive_entry_failed:{error}"))?;
        let name = entry.name().replace('\\', "/");
        if !name.to_ascii_lowercase().ends_with("minecraft.ttf") {
            continue;
        }
        let mut data = Vec::new();
        entry
            .by_ref()
            .take((MAX_FONT_BYTES + 1) as u64)
            .read_to_end(&mut data)
            .map_err(|error| format!("minecraft_font_read_failed:{error}"))?;
        if data.is_empty() || data.len() > MAX_FONT_BYTES {
            return Err("minecraft_font_size_invalid".into());
        }
        font = Some(data);
        break;
    }

    let font = font.ok_or_else(|| "minecraft_font_missing_from_archive".to_string())?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(font);
    Ok(format!("data:font/ttf;base64,{encoded}"))
}
