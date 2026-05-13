use std::sync::Arc;
use tokio::sync::broadcast;

use crate::bilibili::web_live::LiveConnection;
use crate::chat::event::ChatEvent;
use crate::config::ConfigStore;

const CHANNEL_CAPACITY: usize = 64;

const AVATAR_COLORS: &[&str] = &[
    "#e74c3c", "#3498db", "#2ecc71", "#f39c12", "#9b59b6", "#1abc9c", "#e67e22", "#34495e",
];

pub fn avatar_color(uid: u64) -> String {
    AVATAR_COLORS[(uid as usize) % AVATAR_COLORS.len()].to_string()
}

#[derive(Clone)]
pub struct SharedState {
    pub panel_tx: broadcast::Sender<String>,
    pub overlay_tx: broadcast::Sender<String>,
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

pub fn chat_event_to_overlay(event: &ChatEvent) -> serde_json::Value {
    match event {
        ChatEvent::Normal { sender, text, uid } => serde_json::json!({
            "type": "normal",
            "sender": sender,
            "text": text,
            "avatar_color": avatar_color(*uid),
        }),
        ChatEvent::Gift {
            sender,
            gift_name,
            count,
            uid,
        } => serde_json::json!({
            "type": "gift",
            "sender": sender,
            "gift_name": gift_name,
            "count": count,
            "avatar_color": avatar_color(*uid),
        }),
        ChatEvent::SuperChat {
            sender,
            text,
            amount,
            uid,
        } => serde_json::json!({
            "type": "super_chat",
            "sender": sender,
            "text": text,
            "amount": amount,
            "currency": "CNY",
            "avatar_color": avatar_color(*uid),
        }),
        ChatEvent::Guard {
            sender,
            guard_name,
            count,
            uid,
        } => serde_json::json!({
            "type": "guard",
            "sender": sender,
            "guard_name": guard_name,
            "count": count,
            "avatar_color": avatar_color(*uid),
        }),
    }
}

pub fn send_overlay_event(tx: &broadcast::Sender<String>, event: &ChatEvent) {
    let msg = chat_event_to_overlay(event);
    match serde_json::to_string(&msg) {
        Ok(json) => {
            let _ = tx.send(json);
        }
        Err(e) => {
            tracing::error!("failed to serialize overlay event: {e}");
        }
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

    #[test]
    fn test_normal_event_overlay_shape() {
        let event = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "hello".into(),
            uid: 42,
        };
        let msg = chat_event_to_overlay(&event);
        assert_eq!(msg["type"], "normal");
        assert_eq!(msg["sender"], "Alice");
        assert_eq!(msg["text"], "hello");
        assert!(msg["avatar_color"].is_string());
    }

    #[test]
    fn test_gift_event_overlay_shape() {
        let event = ChatEvent::Gift {
            sender: "Bob".into(),
            gift_name: "Flower".into(),
            count: 5,
            uid: 1,
        };
        let msg = chat_event_to_overlay(&event);
        assert_eq!(msg["type"], "gift");
        assert_eq!(msg["sender"], "Bob");
        assert_eq!(msg["gift_name"], "Flower");
        assert_eq!(msg["count"], 5);
    }

    #[test]
    fn test_super_chat_event_overlay_shape() {
        let event = ChatEvent::SuperChat {
            sender: "Carol".into(),
            text: "go!".into(),
            amount: 30,
            uid: 7,
        };
        let msg = chat_event_to_overlay(&event);
        assert_eq!(msg["type"], "super_chat");
        assert_eq!(msg["sender"], "Carol");
        assert_eq!(msg["text"], "go!");
        assert_eq!(msg["amount"], 30);
        assert_eq!(msg["currency"], "CNY");
    }

    #[test]
    fn test_guard_event_overlay_shape() {
        let event = ChatEvent::Guard {
            sender: "Dave".into(),
            guard_name: "Captain".into(),
            count: 1,
            uid: 3,
        };
        let msg = chat_event_to_overlay(&event);
        assert_eq!(msg["type"], "guard");
        assert_eq!(msg["sender"], "Dave");
        assert_eq!(msg["guard_name"], "Captain");
        assert_eq!(msg["count"], 1);
    }

    #[test]
    fn test_send_overlay_event_broadcasts() {
        let (tx, mut rx) = broadcast::channel::<String>(16);
        let event = ChatEvent::Normal {
            sender: "Test".into(),
            text: "msg".into(),
            uid: 0,
        };
        send_overlay_event(&tx, &event);
        let received = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&received).unwrap();
        assert_eq!(parsed["type"], "normal");
        assert_eq!(parsed["sender"], "Test");
    }
}
