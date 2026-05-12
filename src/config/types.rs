use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayOptions {
    #[serde(default = "default_max_items")]
    pub max_items: usize,
    #[serde(default = "default_message_lifetime_secs")]
    pub message_lifetime_secs: u64,
    #[serde(default = "default_show_avatar")]
    pub show_avatar: bool,
}

fn default_max_items() -> usize {
    50
}

fn default_message_lifetime_secs() -> u64 {
    300
}

fn default_show_avatar() -> bool {
    true
}

impl Default for OverlayOptions {
    fn default() -> Self {
        Self {
            max_items: default_max_items(),
            message_lifetime_secs: default_message_lifetime_secs(),
            show_avatar: default_show_avatar(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilterOptions {
    #[serde(default)]
    pub blocked_users: Vec<String>,
    #[serde(default)]
    pub blocked_keywords: Vec<String>,
}
