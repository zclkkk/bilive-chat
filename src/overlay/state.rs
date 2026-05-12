use tokio::sync::broadcast;
use tokio::time::{interval, Duration};

const CHANNEL_CAPACITY: usize = 16;

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

pub fn spawn_synthetic_messages(state: SharedState) {
    let panel_tx = state.panel_tx.clone();
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(5));
        let mut count: u64 = 0;
        loop {
            tick.tick().await;
            count += 1;
            let msg = serde_json::json!({
                "type": "status",
                "status": "waiting",
                "message": format!("connected — waiting for events ({count})")
            });
            if panel_tx.send(msg.to_string()).is_err() {
                break;
            }
        }
    });

    let overlay_tx = state.overlay_tx.clone();
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(3));
        let mut count: u64 = 0;
        loop {
            tick.tick().await;
            count += 1;
            let msg = serde_json::json!({
                "type": "display",
                "kind": "system",
                "text": format!("system event #{count}")
            });
            if overlay_tx.send(msg.to_string()).is_err() {
                break;
            }
        }
    });
}
