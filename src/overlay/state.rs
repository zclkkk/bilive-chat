use std::sync::Arc;
use tokio::sync::broadcast;

use crate::bilibili::web_live::LiveConnection;
use crate::config::ConfigStore;

use super::event::{OverlayEvent, PanelEvent};

const CHANNEL_CAPACITY: usize = 64;

const AVATAR_COLORS: &[&str] = &[
    "#e74c3c", "#3498db", "#2ecc71", "#f39c12", "#9b59b6", "#1abc9c", "#e67e22", "#34495e",
];

pub fn avatar_color(uid: u64) -> String {
    AVATAR_COLORS[(uid as usize) % AVATAR_COLORS.len()].to_string()
}

#[derive(Clone)]
pub struct SharedState {
    pub panel_tx: broadcast::Sender<PanelEvent>,
    pub overlay_tx: broadcast::Sender<OverlayEvent>,
}

#[derive(Clone)]
pub struct AppState {
    pub shared: SharedState,
    pub store: Arc<ConfigStore>,
    pub live: Arc<LiveConnection>,
}

pub fn new() -> SharedState {
    let (panel_tx, _) = broadcast::channel(CHANNEL_CAPACITY);
    let (overlay_tx, _) = broadcast::channel(CHANNEL_CAPACITY);
    SharedState {
        panel_tx,
        overlay_tx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_avatar_color_deterministic() {
        let c1 = avatar_color(42);
        let c2 = avatar_color(42);
        assert_eq!(c1, c2);
        assert!(c1.starts_with('#'));
    }

    #[test]
    fn test_avatar_color_different_uids() {
        let c1 = avatar_color(0);
        let c2 = avatar_color(1);
        assert_ne!(c1, c2);
    }
}
