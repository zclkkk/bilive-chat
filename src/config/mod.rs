pub mod types;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use types::OverlayOptions;

pub use types::FilterOptions;

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

pub struct ConfigStore {
    data_dir: PathBuf,
    pub config: Mutex<Config>,
    pub login_state: Mutex<LoginState>,
}

impl ConfigStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            config: Mutex::new(Config::default()),
            login_state: Mutex::new(LoginState::default()),
        }
    }

    pub fn load_config(&self) -> Result<(), String> {
        let path = self.data_dir.join("config.json");
        match std::fs::read_to_string(&path) {
            Ok(data) => {
                let config: Config =
                    serde_json::from_str(&data).map_err(|e| format!("invalid config: {e}"))?;
                config.validate()?;
                *self.config.lock().unwrap() = config;
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("failed to read config: {e}")),
        }
    }

    pub fn save_config(&self, config: &Config) -> Result<(), String> {
        let path = self.data_dir.join("config.json");
        atomic_write(
            &path,
            &serde_json::to_string_pretty(config).map_err(|e| e.to_string())?,
        )?;
        *self.config.lock().unwrap() = config.clone();
        Ok(())
    }

    pub fn load_login_state(&self) -> Result<(), String> {
        let path = self.data_dir.join("login-state.json");
        match std::fs::read_to_string(&path) {
            Ok(data) => {
                let state: LoginState =
                    serde_json::from_str(&data).map_err(|e| format!("invalid login state: {e}"))?;
                *self.login_state.lock().unwrap() = state;
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("failed to read login state: {e}")),
        }
    }

    pub fn save_login_state(&self, state: &LoginState) -> Result<(), String> {
        let path = self.data_dir.join("login-state.json");
        atomic_write(
            &path,
            &serde_json::to_string_pretty(state).map_err(|e| e.to_string())?,
        )?;
        *self.login_state.lock().unwrap() = state.clone();
        Ok(())
    }

    pub fn delete_login_state(&self) -> Result<(), String> {
        let path = self.data_dir.join("login-state.json");
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }?;
        *self.login_state.lock().unwrap() = LoginState::default();
        Ok(())
    }
}

fn atomic_write(path: &Path, data: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}
