pub mod types;

use serde::{Deserialize, Serialize};
use std::path::Path;
use types::{FilterOptions, OverlayOptions};

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
    pub fn load(data_dir: &Path) -> Result<Self, String> {
        let path = data_dir.join("config.json");
        match std::fs::read_to_string(&path) {
            Ok(data) => {
                let config: Config =
                    serde_json::from_str(&data).map_err(|e| format!("invalid config: {e}"))?;
                config.validate()?;
                Ok(config)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("failed to read config: {e}")),
        }
    }

    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        let path = data_dir.join("config.json");
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, data).map_err(|e| e.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.host.is_empty() {
            return Err("host must not be empty".into());
        }
        if self.port == 0 {
            return Err("port must not be 0".into());
        }
        if self.overlay.max_items == 0 {
            return Err("overlay.max_items must not be 0".into());
        }
        if self.overlay.message_lifetime_secs == 0 {
            return Err("overlay.message_lifetime_secs must not be 0".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoginState {
    pub cookie: String,
    pub updated: Option<String>,
}

impl LoginState {
    pub fn load(data_dir: &Path) -> Result<Self, String> {
        let path = data_dir.join("login-state.json");
        match std::fs::read_to_string(&path) {
            Ok(data) => {
                serde_json::from_str(&data).map_err(|e| format!("invalid login state: {e}"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("failed to read login state: {e}")),
        }
    }

    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        let path = data_dir.join("login-state.json");
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, data).map_err(|e| e.to_string())
    }

    pub fn delete(data_dir: &Path) -> Result<(), String> {
        let path = data_dir.join("login-state.json");
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}
