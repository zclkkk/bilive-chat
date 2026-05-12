pub mod types;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use types::{FilterOptions, OverlayOptions};

fn data_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from("data");
    path.push(name);
    path
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub room_id: u64,
    #[serde(default)]
    pub overlay: OverlayOptions,
    #[serde(default)]
    pub filter: FilterOptions,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    7792
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            room_id: 0,
            overlay: OverlayOptions::default(),
            filter: FilterOptions::default(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = data_path("config.json");
        match std::fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = data_path("config.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, data).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoginState {
    #[serde(default)]
    pub cookie: String,
    #[serde(default)]
    pub updated: Option<String>,
}

impl LoginState {
    pub fn load() -> Self {
        let path = data_path("login-state.json");
        match std::fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = data_path("login-state.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, data).map_err(|e| e.to_string())
    }

    pub fn delete() -> Result<(), String> {
        let path = data_path("login-state.json");
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}
