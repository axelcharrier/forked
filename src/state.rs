//! Remembers open tabs and recent repositories between launches.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 15;

#[derive(Default, Serialize, Deserialize)]
pub struct AppState {
    pub open: Vec<PathBuf>,
    pub active: usize,
    pub recent: Vec<PathBuf>,
}

impl AppState {
    fn file() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("Forked").join("state.json"))
    }

    pub fn load() -> Self {
        Self::file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(file) = Self::file() else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(file, json);
        }
    }

    pub fn add_recent(&mut self, path: PathBuf) {
        self.recent.retain(|p| *p != path);
        self.recent.insert(0, path);
        self.recent.truncate(MAX_RECENT);
    }
}
