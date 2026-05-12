use serde::Serialize;
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};

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

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum DisplayEvent {
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
        currency: String,
        avatar_color: String,
    },
    #[serde(rename = "guard")]
    Guard {
        sender: String,
        guard_name: String,
        count: u32,
        avatar_color: String,
    },
    #[serde(rename = "system")]
    System { text: String },
}

const USERNAMES: &[&str] = &[
    "Alice", "Bob", "Charlie", "Diana", "Eve", "Frank", "Grace", "Hank", "Ivy", "Jack", "Kate",
    "Leo", "Mia", "Noah", "Olivia", "Paul",
];

const NORMAL_MESSAGES: &[&str] = &[
    "Hello!",
    "Nice stream!",
    "LOL",
    "Let's go!",
    "Pog",
    "Haha",
    "First time here, love it!",
    "GG",
    "Wow",
    "Amazing",
    "Hi from Japan",
    "Support!",
    "Good vibes",
    "Clap clap",
    "Based",
    "W streamer",
];

const GIFTS: &[(&str, u32)] = &[
    ("Star", 1),
    ("Heart", 1),
    ("Flower", 5),
    ("Balloon", 10),
    ("Cake", 50),
    ("Diamond", 100),
];

const SUPER_CHAT_AMOUNTS: &[u32] = &[30, 50, 100, 200, 500];
const SUPER_CHAT_TEXTS: &[&str] = &[
    "Go go go!",
    "Love this stream!",
    "Keep it up!",
    "Fighting!",
    "You're the best!",
];

const GUARDS: &[(&str, u32)] = &[
    ("Room Guard", 1),
    ("Room Guard", 3),
    ("Captain", 1),
    ("Admiral", 1),
];

const AVATAR_COLORS: &[&str] = &[
    "#e74c3c", "#3498db", "#2ecc71", "#f39c12", "#9b59b6", "#1abc9c", "#e67e22", "#34495e",
];

fn pick<'a>(list: &'a [&str], index: usize) -> &'a str {
    list[index % list.len()]
}

pub fn spawn_synthetic_messages(state: SharedState) {
    let overlay_tx = state.overlay_tx.clone();
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(3));
        let mut i: usize = 0;
        loop {
            tick.tick().await;
            let event = DisplayEvent::Normal {
                sender: pick(USERNAMES, i).to_string(),
                text: pick(NORMAL_MESSAGES, i + 3).to_string(),
                avatar_color: pick(AVATAR_COLORS, i).to_string(),
            };
            let _ = overlay_tx.send(serde_json::to_string(&event).unwrap());
            i += 1;
        }
    });

    let overlay_tx = state.overlay_tx.clone();
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(7));
        let mut i: usize = 100;
        loop {
            tick.tick().await;
            let (gift_name, count) = GIFTS[i % GIFTS.len()];
            let event = DisplayEvent::Gift {
                sender: pick(USERNAMES, i).to_string(),
                gift_name: gift_name.to_string(),
                count,
                avatar_color: pick(AVATAR_COLORS, i).to_string(),
            };
            let _ = overlay_tx.send(serde_json::to_string(&event).unwrap());
            i += 1;
        }
    });

    let overlay_tx = state.overlay_tx.clone();
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(13));
        let mut i: usize = 200;
        loop {
            tick.tick().await;
            let event = DisplayEvent::SuperChat {
                sender: pick(USERNAMES, i).to_string(),
                text: pick(SUPER_CHAT_TEXTS, i).to_string(),
                amount: SUPER_CHAT_AMOUNTS[i % SUPER_CHAT_AMOUNTS.len()],
                currency: "CNY".to_string(),
                avatar_color: pick(AVATAR_COLORS, i).to_string(),
            };
            let _ = overlay_tx.send(serde_json::to_string(&event).unwrap());
            i += 1;
        }
    });

    let overlay_tx = state.overlay_tx.clone();
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(17));
        let mut i: usize = 300;
        loop {
            tick.tick().await;
            let (guard_name, count) = GUARDS[i % GUARDS.len()];
            let event = DisplayEvent::Guard {
                sender: pick(USERNAMES, i).to_string(),
                guard_name: guard_name.to_string(),
                count,
                avatar_color: pick(AVATAR_COLORS, i).to_string(),
            };
            let _ = overlay_tx.send(serde_json::to_string(&event).unwrap());
            i += 1;
        }
    });

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
            let _ = panel_tx.send(msg.to_string());
        }
    });

    let overlay_tx = state.overlay_tx.clone();
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(30));
        let mut count: u64 = 0;
        loop {
            tick.tick().await;
            count += 1;
            let event = DisplayEvent::System {
                text: format!("system event #{count}"),
            };
            let _ = overlay_tx.send(serde_json::to_string(&event).unwrap());
        }
    });
}
