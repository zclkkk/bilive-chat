use serde::Serialize;

use crate::bilibili::web_live::SocketStatus;
use crate::chat::event::ChatEvent;

use super::state::avatar_color;

#[derive(Clone, Serialize)]
#[serde(tag = "type")]
pub enum PanelEvent {
    #[serde(rename = "status")]
    Status { status: SocketStatus },
}

#[derive(Clone, Serialize)]
#[serde(tag = "type")]
pub enum OverlayEvent {
    #[serde(rename = "normal")]
    Normal {
        sender: String,
        text: String,
        avatar_color: String,
    },
    #[serde(rename = "gift")]
    Gift {
        sender: String,
        gift_name: String,
        count: u32,
        avatar_color: String,
    },
    #[serde(rename = "super_chat")]
    SuperChat {
        sender: String,
        text: String,
        amount: u32,
        currency: &'static str,
        avatar_color: String,
    },
    #[serde(rename = "guard")]
    Guard {
        sender: String,
        guard_name: String,
        count: u32,
        avatar_color: String,
    },
}

impl From<&ChatEvent> for OverlayEvent {
    fn from(event: &ChatEvent) -> Self {
        match event {
            ChatEvent::Normal { sender, text, uid } => OverlayEvent::Normal {
                sender: sender.clone(),
                text: text.clone(),
                avatar_color: avatar_color(*uid),
            },
            ChatEvent::Gift {
                sender,
                gift_name,
                count,
                uid,
            } => OverlayEvent::Gift {
                sender: sender.clone(),
                gift_name: gift_name.clone(),
                count: *count,
                avatar_color: avatar_color(*uid),
            },
            ChatEvent::SuperChat {
                sender,
                text,
                amount,
                uid,
            } => OverlayEvent::SuperChat {
                sender: sender.clone(),
                text: text.clone(),
                amount: *amount,
                currency: "CNY",
                avatar_color: avatar_color(*uid),
            },
            ChatEvent::Guard {
                sender,
                guard_name,
                count,
                uid,
            } => OverlayEvent::Guard {
                sender: sender.clone(),
                guard_name: guard_name.clone(),
                count: *count,
                avatar_color: avatar_color(*uid),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_event_overlay_shape() {
        let event = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "hello".into(),
            uid: 42,
        };
        let overlay = OverlayEvent::from(&event);
        let json = serde_json::to_value(&overlay).unwrap();
        assert_eq!(json["type"], "normal");
        assert_eq!(json["sender"], "Alice");
        assert_eq!(json["text"], "hello");
        assert!(json["avatar_color"].is_string());
    }

    #[test]
    fn test_gift_event_overlay_shape() {
        let event = ChatEvent::Gift {
            sender: "Bob".into(),
            gift_name: "Flower".into(),
            count: 5,
            uid: 1,
        };
        let overlay = OverlayEvent::from(&event);
        let json = serde_json::to_value(&overlay).unwrap();
        assert_eq!(json["type"], "gift");
        assert_eq!(json["sender"], "Bob");
        assert_eq!(json["gift_name"], "Flower");
        assert_eq!(json["count"], 5);
    }

    #[test]
    fn test_super_chat_event_overlay_shape() {
        let event = ChatEvent::SuperChat {
            sender: "Carol".into(),
            text: "go!".into(),
            amount: 30,
            uid: 7,
        };
        let overlay = OverlayEvent::from(&event);
        let json = serde_json::to_value(&overlay).unwrap();
        assert_eq!(json["type"], "super_chat");
        assert_eq!(json["sender"], "Carol");
        assert_eq!(json["text"], "go!");
        assert_eq!(json["amount"], 30);
        assert_eq!(json["currency"], "CNY");
    }

    #[test]
    fn test_guard_event_overlay_shape() {
        let event = ChatEvent::Guard {
            sender: "Dave".into(),
            guard_name: "Captain".into(),
            count: 1,
            uid: 3,
        };
        let overlay = OverlayEvent::from(&event);
        let json = serde_json::to_value(&overlay).unwrap();
        assert_eq!(json["type"], "guard");
        assert_eq!(json["sender"], "Dave");
        assert_eq!(json["guard_name"], "Captain");
        assert_eq!(json["count"], 1);
    }

    #[test]
    fn test_panel_status_event_shape() {
        let event = PanelEvent::Status {
            status: SocketStatus::Connected {},
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "status");
        assert_eq!(json["status"]["type"], "connected");
    }
}
