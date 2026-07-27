use crate::minecraft::launcher::{LaunchStatus, RunningInstance};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

#[derive(Clone)]
pub struct AppState {
    pub children: Arc<Mutex<Vec<RunningInstance>>>,
    pub launch: Arc<RwLock<LaunchStatus>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            children: Arc::new(Mutex::new(Vec::new())),
            launch: Arc::new(RwLock::new(LaunchStatus::idle())),
        }
    }
}
