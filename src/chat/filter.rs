use crate::chat::event::ChatEvent;
use crate::config::types::FilterOptions;

pub struct ChatFilter {
    blocked_users: Vec<String>,
    blocked_keywords: Vec<String>,
}

fn normalize_entries(entries: &[String]) -> Vec<String> {
    entries
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

impl ChatFilter {
    pub fn new(options: &FilterOptions) -> Self {
        Self {
            blocked_users: normalize_entries(&options.blocked_users),
            blocked_keywords: normalize_entries(&options.blocked_keywords),
        }
    }

    pub fn should_block(&self, event: &ChatEvent) -> bool {
        let sender = event.sender();
        if self.blocked_users.iter().any(|u| u == sender) {
            return true;
        }
        let text = event.searchable_text();
        if self
            .blocked_keywords
            .iter()
            .any(|kw| text.contains(kw.as_str()))
        {
            return true;
        }
        false
    }
}

trait Filterable {
    fn sender(&self) -> &str;
    fn searchable_text(&self) -> &str;
}

impl Filterable for ChatEvent {
    fn sender(&self) -> &str {
        match self {
            ChatEvent::Normal { sender, .. } => sender,
            ChatEvent::Gift { sender, .. } => sender,
            ChatEvent::SuperChat { sender, .. } => sender,
            ChatEvent::Guard { sender, .. } => sender,
        }
    }

    fn searchable_text(&self) -> &str {
        match self {
            ChatEvent::Normal { text, .. } => text,
            ChatEvent::SuperChat { text, .. } => text,
            ChatEvent::Gift { .. } => "",
            ChatEvent::Guard { .. } => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(users: &[&str], keywords: &[&str]) -> ChatFilter {
        ChatFilter::new(&FilterOptions {
            blocked_users: users.iter().map(|s| s.to_string()).collect(),
            blocked_keywords: keywords.iter().map(|s| s.to_string()).collect(),
        })
    }

    #[test]
    fn test_empty_filter_passes_all() {
        let f = filter(&[], &[]);
        let event = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "hello".into(),
            uid: 1,
        };
        assert!(!f.should_block(&event));
    }

    #[test]
    fn test_blocked_user_normal() {
        let f = filter(&["Spammer"], &[]);
        let event = ChatEvent::Normal {
            sender: "Spammer".into(),
            text: "hello".into(),
            uid: 1,
        };
        assert!(f.should_block(&event));
    }

    #[test]
    fn test_blocked_user_gift() {
        let f = filter(&["Troll"], &[]);
        let event = ChatEvent::Gift {
            sender: "Troll".into(),
            gift_name: "Flower".into(),
            count: 1,
            uid: 2,
        };
        assert!(f.should_block(&event));
    }

    #[test]
    fn test_blocked_user_super_chat() {
        let f = filter(&["BadUser"], &[]);
        let event = ChatEvent::SuperChat {
            sender: "BadUser".into(),
            text: "hi".into(),
            amount: 30,
            uid: 3,
        };
        assert!(f.should_block(&event));
    }

    #[test]
    fn test_blocked_user_guard() {
        let f = filter(&["Guardian"], &[]);
        let event = ChatEvent::Guard {
            sender: "Guardian".into(),
            guard_name: "Captain".into(),
            count: 1,
            uid: 4,
        };
        assert!(f.should_block(&event));
    }

    #[test]
    fn test_non_blocked_user_passes() {
        let f = filter(&["Spammer"], &[]);
        let event = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "hello".into(),
            uid: 1,
        };
        assert!(!f.should_block(&event));
    }

    #[test]
    fn test_blocked_keyword_normal() {
        let f = filter(&[], &["bad"]);
        let event = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "this is bad".into(),
            uid: 1,
        };
        assert!(f.should_block(&event));
    }

    #[test]
    fn test_blocked_keyword_super_chat() {
        let f = filter(&[], &["spam"]);
        let event = ChatEvent::SuperChat {
            sender: "Alice".into(),
            text: "spam message".into(),
            amount: 30,
            uid: 2,
        };
        assert!(f.should_block(&event));
    }

    #[test]
    fn test_blocked_keyword_does_not_match_gift() {
        let f = filter(&[], &["bad"]);
        let event = ChatEvent::Gift {
            sender: "Alice".into(),
            gift_name: "Flower".into(),
            count: 1,
            uid: 3,
        };
        assert!(!f.should_block(&event));
    }

    #[test]
    fn test_blocked_keyword_does_not_match_guard() {
        let f = filter(&[], &["bad"]);
        let event = ChatEvent::Guard {
            sender: "Alice".into(),
            guard_name: "Captain".into(),
            count: 1,
            uid: 4,
        };
        assert!(!f.should_block(&event));
    }

    #[test]
    fn test_keyword_substring_match() {
        let f = filter(&[], &["abc"]);
        let event = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "xxxabcyyy".into(),
            uid: 1,
        };
        assert!(f.should_block(&event));
    }

    #[test]
    fn test_keyword_case_sensitive() {
        let f = filter(&[], &["bad"]);
        let event = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "BAD".into(),
            uid: 1,
        };
        assert!(!f.should_block(&event));
    }

    #[test]
    fn test_user_match_is_exact() {
        let f = filter(&["Spam"], &[]);
        let event = ChatEvent::Normal {
            sender: "Spammer".into(),
            text: "hello".into(),
            uid: 1,
        };
        assert!(!f.should_block(&event));
    }

    #[test]
    fn test_both_user_and_keyword() {
        let f = filter(&["Spammer"], &["bad"]);
        let event_user = ChatEvent::Normal {
            sender: "Spammer".into(),
            text: "ok".into(),
            uid: 1,
        };
        let event_kw = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "bad".into(),
            uid: 2,
        };
        let event_ok = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "ok".into(),
            uid: 3,
        };
        assert!(f.should_block(&event_user));
        assert!(f.should_block(&event_kw));
        assert!(!f.should_block(&event_ok));
    }

    #[test]
    fn test_empty_keyword_does_not_block_normal() {
        let f = filter(&[], &[""]);
        let event = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "hello".into(),
            uid: 1,
        };
        assert!(!f.should_block(&event));
    }

    #[test]
    fn test_whitespace_keyword_does_not_block_normal() {
        let f = filter(&[], &["  "]);
        let event = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "hello".into(),
            uid: 1,
        };
        assert!(!f.should_block(&event));
    }

    #[test]
    fn test_empty_user_does_not_block() {
        let f = filter(&[""], &[]);
        let event = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "hello".into(),
            uid: 1,
        };
        assert!(!f.should_block(&event));
    }

    #[test]
    fn test_whitespace_user_does_not_block() {
        let f = filter(&["  "], &[]);
        let event = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "hello".into(),
            uid: 1,
        };
        assert!(!f.should_block(&event));
    }

    #[test]
    fn test_trimmed_keyword_matches() {
        let f = filter(&[], &["  bad  "]);
        let event = ChatEvent::Normal {
            sender: "Alice".into(),
            text: "this is bad".into(),
            uid: 1,
        };
        assert!(f.should_block(&event));
    }

    #[test]
    fn test_trimmed_user_matches() {
        let f = filter(&["  Spammer  "], &[]);
        let event = ChatEvent::Normal {
            sender: "Spammer".into(),
            text: "hello".into(),
            uid: 1,
        };
        assert!(f.should_block(&event));
    }
}
