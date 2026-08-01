use std::collections::{HashSet, VecDeque};

use crate::youtube_api_v3::LiveChatMessage;
use crate::youtube_api_v3::live_chat_message_snippet::type_wrapper::Type as MessageType;

/// Remembers recently seen message ids so a page-token replay (which happens
/// whenever we resume a stream) cannot print the same message twice.
pub struct MessageDedup {
    seen: HashSet<String>,
    order: VecDeque<String>,
    capacity: usize,
}

impl MessageDedup {
    pub fn new(capacity: usize) -> Self {
        Self {
            seen: HashSet::with_capacity(capacity),
            order: VecDeque::with_capacity(capacity),
            capacity: capacity.max(1),
        }
    }

    /// Returns true the first time an id is seen, false for repeats. Messages
    /// without an id are always let through -- dropping them would be worse
    /// than showing a rare duplicate.
    pub fn insert(&mut self, id: Option<&str>) -> bool {
        let Some(id) = id.filter(|id| !id.is_empty()) else {
            return true;
        };

        if !self.seen.insert(id.to_string()) {
            return false;
        }

        self.order.push_back(id.to_string());
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.seen.remove(&oldest);
            }
        }

        true
    }
}

impl Default for MessageDedup {
    fn default() -> Self {
        Self::new(2_000)
    }
}

/// True when the item means the broadcast's chat is over.
pub fn is_chat_ended(item: &LiveChatMessage) -> bool {
    item.snippet
        .as_ref()
        .is_some_and(|s| s.r#type() == MessageType::ChatEndedEvent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::youtube_api_v3::LiveChatMessageSnippet;

    #[test]
    fn dedup_rejects_repeats_and_keeps_new_ids() {
        let mut dedup = MessageDedup::new(3);
        assert!(dedup.insert(Some("a")));
        assert!(!dedup.insert(Some("a")));
        assert!(dedup.insert(Some("b")));
        assert!(!dedup.insert(Some("b")));
    }

    #[test]
    fn dedup_evicts_oldest_beyond_capacity() {
        let mut dedup = MessageDedup::new(2);
        dedup.insert(Some("a"));
        dedup.insert(Some("b"));
        dedup.insert(Some("c"));
        // "a" was evicted, so it is treated as new again.
        assert!(dedup.insert(Some("a")));
        // "c" is still remembered.
        assert!(!dedup.insert(Some("c")));
    }

    #[test]
    fn dedup_lets_id_less_messages_through() {
        let mut dedup = MessageDedup::new(4);
        assert!(dedup.insert(None));
        assert!(dedup.insert(None));
        assert!(dedup.insert(Some("")));
    }

    #[test]
    fn chat_ended_is_detected() {
        let ended = LiveChatMessage {
            snippet: Some(LiveChatMessageSnippet {
                r#type: Some(MessageType::ChatEndedEvent as i32),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(is_chat_ended(&ended));

        let text = LiveChatMessage {
            snippet: Some(LiveChatMessageSnippet {
                r#type: Some(MessageType::TextMessageEvent as i32),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(!is_chat_ended(&text));

        assert!(!is_chat_ended(&LiveChatMessage::default()));
    }
}
