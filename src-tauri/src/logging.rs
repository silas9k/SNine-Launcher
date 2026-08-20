use crate::{app::paths, error::AppResult};
use chrono::Local;
use regex::Regex;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    sync::LazyLock,
};

static SENSITIVE_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r#"(?i)(authorization\s*[:=]\s*bearer\s+)[^\s,;\"']+"#,
        r#"(?i)((?:access|refresh)[_-]?token\s*[:=]\s*[\"']?)[^\s,;\"']+"#,
        r#"(?i)(device[_-]?code\s*[:=]\s*[\"']?)[^\s,;\"']+"#,
        r#"(?i)(identitytoken\s*[:=]\s*[\"']?)[^\s,;\"']+"#,
        r#"(?i)(rpsticket\s*[:=]\s*[\"']?d=)[^\s,;\"']+"#,
    ]
    .into_iter()
    .map(|pattern| Regex::new(pattern).expect("static redaction expression"))
    .collect()
});

pub fn redact_sensitive(message: &str) -> String {
    SENSITIVE_PATTERNS
        .iter()
        .fold(message.to_string(), |value, pattern| {
            pattern.replace_all(&value, "${1}[REDACTED]").into_owned()
        })
}

pub fn append(message: &str) -> AppResult<()> {
    let path = paths::launcher_paths()?.log_file;
    if path.exists() && fs::metadata(&path)?.len() > 2_000_000 {
        let rotated = path.with_extension("old.log");
        let _ = fs::remove_file(&rotated);
        fs::rename(&path, rotated)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let message = redact_sensitive(message);
    writeln!(
        file,
        "[{}] {}",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        message
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_all_authentication_material_from_log_lines() {
        let access = ["access", "-fixture-value"].concat();
        let refresh = ["refresh", "-fixture-value"].concat();
        let device = ["device", "-fixture-value"].concat();
        let line = format!(
            "Authorization: Bearer {access}; refresh_token={refresh}; device_code={device}"
        );
        let sanitized = redact_sensitive(&line);
        for secret in [access, refresh, device] {
            assert!(!sanitized.contains(&secret));
        }
        assert_eq!(sanitized.matches("[REDACTED]").count(), 3);
    }
}
