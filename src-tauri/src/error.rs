use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Serialize)]
pub struct ErrorDescriptor {
    pub code: String,
    pub message_key: String,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("{message_key}")]
    Coded {
        code: String,
        message_key: String,
        params: BTreeMap<String, String>,
    },
    #[error("Dateifehler: {0}")]
    Io(#[from] std::io::Error),
    #[error("Netzwerkfehler: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Ungültige Daten: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZIP-Fehler: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("SQLite-Fehler: {0}")]
    Sqlite(String),
    #[error("Account wurde nicht gefunden: {0}")]
    AccountNotFound(String),
    #[error("S9Lab Client ist noch nicht installiert")]
    ClientNotInstalled,
    #[error("Datei-Prüfsumme stimmt nicht: {path:?}")]
    HashMismatch { path: PathBuf },
}

impl AppError {
    pub fn coded(code: impl Into<String>) -> Self {
        let code = code.into();
        Self::Coded {
            message_key: format!("error.{code}"),
            code,
            params: BTreeMap::new(),
        }
    }

    pub fn coded_with(
        code: impl Into<String>,
        params: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        let code = code.into();
        Self::Coded {
            message_key: format!("error.{code}"),
            code,
            params: params
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }

    pub fn descriptor(&self) -> ErrorDescriptor {
        match self {
            Self::Coded {
                code,
                message_key,
                params,
            } => ErrorDescriptor {
                code: code.clone(),
                message_key: message_key.clone(),
                params: params.clone(),
            },
            Self::Io(error) => ErrorDescriptor {
                code: "io_error".into(),
                message_key: "error.io_error".into(),
                params: BTreeMap::from([("detail".into(), error.to_string())]),
            },
            Self::Http(error) => ErrorDescriptor {
                code: "network_error".into(),
                message_key: "error.network_error".into(),
                params: BTreeMap::from([("detail".into(), error.to_string())]),
            },
            Self::Json(error) => ErrorDescriptor {
                code: "invalid_json".into(),
                message_key: "error.invalid_json".into(),
                params: BTreeMap::from([("detail".into(), error.to_string())]),
            },
            Self::Zip(error) => ErrorDescriptor {
                code: "invalid_zip".into(),
                message_key: "error.invalid_zip".into(),
                params: BTreeMap::from([("detail".into(), error.to_string())]),
            },
            Self::Sqlite(error) => ErrorDescriptor {
                code: "storage_error".into(),
                message_key: "error.storage_error".into(),
                params: BTreeMap::from([("detail".into(), error.clone())]),
            },
            Self::AccountNotFound(account_id) => ErrorDescriptor {
                code: "account_not_found".into(),
                message_key: "error.account_not_found".into(),
                params: BTreeMap::from([("accountId".into(), account_id.clone())]),
            },
            Self::ClientNotInstalled => ErrorDescriptor {
                code: "client_not_installed".into(),
                message_key: "error.client_not_installed".into(),
                params: BTreeMap::new(),
            },
            Self::HashMismatch { path } => ErrorDescriptor {
                code: "hash_mismatch".into(),
                message_key: "error.hash_mismatch".into(),
                params: BTreeMap::from([("path".into(), path.display().to_string())]),
            },
            Self::Message(message) => ErrorDescriptor {
                code: "internal_error".into(),
                message_key: "error.internal_error".into(),
                params: BTreeMap::from([("detail".into(), message.clone())]),
            },
        }
    }
}

impl From<keyring::Error> for AppError {
    fn from(value: keyring::Error) -> Self {
        Self::coded_with("credential_store_error", [("detail", value.to_string())])
    }
}
