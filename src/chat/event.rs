use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ChatEvent {
    #[serde(rename = "normal")]
    Normal {
        sender: String,
        text: String,
        uid: u64,
    },
    #[serde(rename = "gift")]
    Gift {
        sender: String,
        gift_name: String,
        count: u32,
        uid: u64,
    },
    #[serde(rename = "super_chat")]
    SuperChat {
        sender: String,
        text: String,
        amount: u32,
        uid: u64,
    },
    #[serde(rename = "guard")]
    Guard {
        sender: String,
        guard_name: String,
        count: u32,
        uid: u64,
    },
}
