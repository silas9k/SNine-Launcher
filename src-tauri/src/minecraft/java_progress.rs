use serde::Serialize;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
pub enum JavaStage {
    Downloading,
    Extracting,
    Verifying,
    Ready,
}

#[derive(Clone, Serialize)]
pub struct JavaProgress {
    pub stage: JavaStage,
    pub percent: f64,
    pub speed_mb: f64,
}

pub struct JavaProgressTracker {
    last_bytes: u64,
    last_time: Instant,
    smoothed_speed: f64,
    alpha: f64,
}

impl JavaProgressTracker {
    pub fn new() -> Self {
        Self {
            last_bytes: 0,
            last_time: Instant::now(),
            smoothed_speed: 0.2,
            alpha: 0.2,
        }
    }

    pub fn update(&mut self, app: &AppHandle, stage: JavaStage, bytes: u64, total: u64) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_time).as_secs_f64();

        let instant_speed = if elapsed > 0.0 {
            (bytes - self.last_bytes) as f64 / elapsed
        } else {
            0.0
        };

        self.smoothed_speed = self.smoothed_speed * (1.0 - self.alpha) + instant_speed * self.alpha;

        let percent = if total > 0 {
            (bytes as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let _ = app.emit(
            "java-progress",
            JavaProgress {
                stage,
                percent,
                speed_mb: self.smoothed_speed / 1_048_576.0,
            },
        );

        self.last_bytes = bytes;
        self.last_time = now;
    }
}
