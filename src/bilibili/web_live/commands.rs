use crate::chat::event::ChatEvent;

pub fn parse_command(value: &serde_json::Value) -> Option<ChatEvent> {
    let cmd = value.get("cmd").and_then(|v| v.as_str())?;
    match cmd {
        "DANMU_MSG" | "DANMU_MSG:4:0:2:2:2:0" => parse_danmu_msg(value),
        "SEND_GIFT" => parse_send_gift(value),
        "SUPER_CHAT_MESSAGE" => parse_super_chat_message(value),
        "GUARD_BUY" => parse_guard_buy(value),
        _ => None,
    }
}

fn parse_danmu_msg(value: &serde_json::Value) -> Option<ChatEvent> {
    let info = value.get("info")?.as_array()?;
    let text = info.get(1)?.as_str()?.to_string();
    let sender_array = info.get(2)?.as_array()?;
    let uid = sender_array.first()?.as_u64().unwrap_or(0);
    let sender = sender_array
        .get(1)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if sender.is_empty() {
        return None;
    }
    Some(ChatEvent::Normal { sender, text, uid })
}

fn parse_send_gift(value: &serde_json::Value) -> Option<ChatEvent> {
    let data = value.get("data")?;
    let sender = data
        .get("uname")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let gift_name = data
        .get("giftName")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let count = data.get("num").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
    let uid = data.get("uid").and_then(|v| v.as_u64()).unwrap_or(0);
    Some(ChatEvent::Gift {
        sender,
        gift_name,
        count,
        uid,
    })
}

fn parse_super_chat_message(value: &serde_json::Value) -> Option<ChatEvent> {
    let data = value.get("data")?;
    let user_info = data.get("user_info")?;
    let sender = user_info
        .get("uname")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let text = data
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let amount = data.get("price").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let uid = data.get("uid").and_then(|v| v.as_u64()).unwrap_or(0);
    Some(ChatEvent::SuperChat {
        sender,
        text,
        amount,
        uid,
    })
}

fn parse_guard_buy(value: &serde_json::Value) -> Option<ChatEvent> {
    let data = value.get("data")?;
    let sender = data
        .get("username")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let guard_name = data
        .get("gift_name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let count = data.get("num").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
    let uid = data.get("uid").and_then(|v| v.as_u64()).unwrap_or(0);
    Some(ChatEvent::Guard {
        sender,
        guard_name,
        count,
        uid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_danmu_msg() {
        let msg = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [
                [0, 1, 25, 16777215, 1700000000, 0, 0, "abc", 0, 0, 0, ""],
                "hello world",
                [12345, "Alice", 0, 0, 0, 10000, 1, ""]
            ]
        });
        let event = parse_command(&msg).unwrap();
        match event {
            ChatEvent::Normal { sender, text, uid } => {
                assert_eq!(sender, "Alice");
                assert_eq!(text, "hello world");
                assert_eq!(uid, 12345);
            }
            _ => panic!("expected Normal"),
        }
    }

    #[test]
    fn test_parse_danmu_msg_variant() {
        let msg = serde_json::json!({
            "cmd": "DANMU_MSG:4:0:2:2:2:0",
            "info": [
                [0, 1, 25, 16777215, 1700000000, 0, 0, "abc", 0, 0, 0, ""],
                "variant msg",
                [99, "Bob", 0, 0, 0, 10000, 1, ""]
            ]
        });
        let event = parse_command(&msg).unwrap();
        match event {
            ChatEvent::Normal { sender, text, uid } => {
                assert_eq!(sender, "Bob");
                assert_eq!(text, "variant msg");
                assert_eq!(uid, 99);
            }
            _ => panic!("expected Normal"),
        }
    }

    #[test]
    fn test_parse_send_gift() {
        let msg = serde_json::json!({
            "cmd": "SEND_GIFT",
            "data": {
                "giftId": 123,
                "giftName": "Flower",
                "coin_type": "gold",
                "price": 100,
                "num": 5,
                "uid": 456,
                "uname": "Carol",
                "timestamp": 1700000000
            }
        });
        let event = parse_command(&msg).unwrap();
        match event {
            ChatEvent::Gift {
                sender,
                gift_name,
                count,
                uid,
            } => {
                assert_eq!(sender, "Carol");
                assert_eq!(gift_name, "Flower");
                assert_eq!(count, 5);
                assert_eq!(uid, 456);
            }
            _ => panic!("expected Gift"),
        }
    }

    #[test]
    fn test_parse_super_chat_message() {
        let msg = serde_json::json!({
            "cmd": "SUPER_CHAT_MESSAGE",
            "data": {
                "id": 789,
                "uid": 111,
                "price": 30,
                "message": "Go go go!",
                "user_info": {
                    "uname": "Dave",
                    "uid": 111
                }
            }
        });
        let event = parse_command(&msg).unwrap();
        match event {
            ChatEvent::SuperChat {
                sender,
                text,
                amount,
                uid,
            } => {
                assert_eq!(sender, "Dave");
                assert_eq!(text, "Go go go!");
                assert_eq!(amount, 30);
                assert_eq!(uid, 111);
            }
            _ => panic!("expected SuperChat"),
        }
    }

    #[test]
    fn test_parse_guard_buy() {
        let msg = serde_json::json!({
            "cmd": "GUARD_BUY",
            "data": {
                "uid": 222,
                "username": "Eve",
                "gift_name": "Captain",
                "num": 1,
                "price": 1980
            }
        });
        let event = parse_command(&msg).unwrap();
        match event {
            ChatEvent::Guard {
                sender,
                guard_name,
                count,
                uid,
            } => {
                assert_eq!(sender, "Eve");
                assert_eq!(guard_name, "Captain");
                assert_eq!(count, 1);
                assert_eq!(uid, 222);
            }
            _ => panic!("expected Guard"),
        }
    }

    #[test]
    fn test_unknown_command_returns_none() {
        let msg = serde_json::json!({"cmd": "ROOM_CHANGE", "data": {"title": "new title"}});
        assert!(parse_command(&msg).is_none());
    }

    #[test]
    fn test_no_cmd_field_returns_none() {
        let msg = serde_json::json!({"data": {"foo": "bar"}});
        assert!(parse_command(&msg).is_none());
    }

    #[test]
    fn test_malformed_danmu_msg_returns_none() {
        let msg = serde_json::json!({"cmd": "DANMU_MSG", "info": "not an array"});
        assert!(parse_command(&msg).is_none());
    }

    #[test]
    fn test_danmu_msg_empty_sender_returns_none() {
        let msg = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [
                [0],
                "text",
                [0, "", 0, 0, 0, 0, 1, ""]
            ]
        });
        assert!(parse_command(&msg).is_none());
    }

    #[test]
    fn test_send_gift_empty_uname_returns_none() {
        let msg = serde_json::json!({
            "cmd": "SEND_GIFT",
            "data": {
                "giftName": "Flower",
                "uname": "",
                "num": 1,
                "uid": 1
            }
        });
        assert!(parse_command(&msg).is_none());
    }

    #[test]
    fn test_send_gift_empty_gift_name_returns_none() {
        let msg = serde_json::json!({
            "cmd": "SEND_GIFT",
            "data": {
                "giftName": "",
                "uname": "Alice",
                "num": 1,
                "uid": 1
            }
        });
        assert!(parse_command(&msg).is_none());
    }

    #[test]
    fn test_send_gift_no_data_returns_none() {
        let msg = serde_json::json!({"cmd": "SEND_GIFT"});
        assert!(parse_command(&msg).is_none());
    }

    #[test]
    fn test_super_chat_no_user_info_returns_none() {
        let msg = serde_json::json!({
            "cmd": "SUPER_CHAT_MESSAGE",
            "data": {
                "price": 30,
                "message": "hello"
            }
        });
        assert!(parse_command(&msg).is_none());
    }

    #[test]
    fn test_guard_buy_empty_username_returns_none() {
        let msg = serde_json::json!({
            "cmd": "GUARD_BUY",
            "data": {
                "username": "",
                "gift_name": "Captain",
                "num": 1,
                "uid": 1
            }
        });
        assert!(parse_command(&msg).is_none());
    }

    #[test]
    fn test_guard_buy_empty_gift_name_returns_none() {
        let msg = serde_json::json!({
            "cmd": "GUARD_BUY",
            "data": {
                "username": "Alice",
                "gift_name": "",
                "num": 1,
                "uid": 1
            }
        });
        assert!(parse_command(&msg).is_none());
    }

    #[test]
    fn test_non_object_returns_none() {
        let msg = serde_json::json!("just a string");
        assert!(parse_command(&msg).is_none());
    }

    #[test]
    fn test_danmu_msg_missing_info_field_returns_none() {
        let msg = serde_json::json!({"cmd": "DANMU_MSG"});
        assert!(parse_command(&msg).is_none());
    }

    #[test]
    fn test_danmu_msg_short_info_returns_none() {
        let msg = serde_json::json!({
            "cmd": "DANMU_MSG",
            "info": [[0]]
        });
        assert!(parse_command(&msg).is_none());
    }

    #[test]
    fn test_send_gift_defaults_count_to_one() {
        let msg = serde_json::json!({
            "cmd": "SEND_GIFT",
            "data": {
                "giftName": "Star",
                "uname": "Alice",
                "uid": 1
            }
        });
        let event = parse_command(&msg).unwrap();
        match event {
            ChatEvent::Gift { count, .. } => assert_eq!(count, 1),
            _ => panic!("expected Gift"),
        }
    }

    #[test]
    fn test_super_chat_defaults_amount_to_zero() {
        let msg = serde_json::json!({
            "cmd": "SUPER_CHAT_MESSAGE",
            "data": {
                "message": "hi",
                "user_info": {"uname": "Alice"}
            }
        });
        let event = parse_command(&msg).unwrap();
        match event {
            ChatEvent::SuperChat { amount, .. } => assert_eq!(amount, 0),
            _ => panic!("expected SuperChat"),
        }
    }
}
