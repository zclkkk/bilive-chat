use tokio::sync::broadcast;

const CHANNEL_CAPACITY: usize = 64;

#[derive(Clone)]
pub struct SharedState {
    pub panel_tx: broadcast::Sender<String>,
    pub overlay_tx: broadcast::Sender<String>,
}

pub fn new() -> SharedState {
    let (panel_tx, _) = broadcast::channel(CHANNEL_CAPACITY);
    let (overlay_tx, _) = broadcast::channel(CHANNEL_CAPACITY);
    SharedState {
        panel_tx,
        overlay_tx,
    }
}
