use std::collections::{HashSet, VecDeque};

use crate::app::event::{ChatMessage, MessageKind};
use crate::youtube_api_v3::LiveChatMessage;
use crate::youtube_api_v3::live_chat_message_snippet::DisplayedContent;
use crate::youtube_api_v3::live_chat_message_snippet::type_wrapper::Type as MessageType;

const UNKNOWN_AUTHOR: &str = "<unknown>";
const NO_TIME: &str = "--:--";

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

/// Extract `HH:MM` from an ISO-8601 timestamp without assuming it is present or
/// well formed. `publishedAt` is optional in the proto, so a missing value must
/// never take the chat stream down.
fn format_timestamp(published_at: Option<&str>) -> String {
    published_at
        .and_then(|ts| ts.get(11..16))
        .filter(|hhmm| hhmm.len() == 5 && hhmm.as_bytes()[2] == b':')
        .unwrap_or(NO_TIME)
        .to_string()
}

/// Convert a live chat item into something renderable. Returns `None` for
/// silent messages (`hasDisplayContent == false`) and for types we have nothing
/// to show for.
pub fn map_message(item: &LiveChatMessage) -> Option<ChatMessage> {
    let snippet = item.snippet.as_ref()?;
    let author_details = item.author_details.as_ref();

    let author = author_details
        .and_then(|d| d.display_name.as_deref())
        .filter(|name| !name.is_empty())
        .unwrap_or(UNKNOWN_AUTHOR)
        .to_string();

    let is_member = author_details
        .and_then(|d| d.is_chat_sponsor)
        .unwrap_or(false);

    let avatar_url = author_details
        .and_then(|d| d.profile_image_url.as_deref())
        .filter(|url| !url.is_empty())
        .map(str::to_string);

    let display_message = snippet.display_message.as_deref().filter(|m| !m.is_empty());

    let (kind, message) = match snippet.r#type() {
        MessageType::TextMessageEvent => {
            let text = match snippet.displayed_content.as_ref() {
                Some(DisplayedContent::TextMessageDetails(d)) => d.message_text.as_deref(),
                _ => None,
            }
            .or(display_message)?;

            (MessageKind::Text, text.to_string())
        }

        MessageType::SuperChatEvent => match snippet.displayed_content.as_ref() {
            Some(DisplayedContent::SuperChatDetails(d)) => {
                let amount = d
                    .amount_display_string
                    .clone()
                    .unwrap_or_else(|| "Super Chat".to_string());
                let text = d
                    .user_comment
                    .as_deref()
                    .filter(|c| !c.is_empty())
                    .or(display_message)
                    .unwrap_or("")
                    .to_string();
                (MessageKind::SuperChat { amount }, text)
            }
            _ => (
                MessageKind::SuperChat {
                    amount: "Super Chat".to_string(),
                },
                display_message.unwrap_or("").to_string(),
            ),
        },

        MessageType::SuperStickerEvent => match snippet.displayed_content.as_ref() {
            Some(DisplayedContent::SuperStickerDetails(d)) => {
                let amount = d
                    .amount_display_string
                    .clone()
                    .unwrap_or_else(|| "Super Sticker".to_string());
                (
                    MessageKind::SuperChat { amount },
                    display_message
                        .unwrap_or("sent a Super Sticker")
                        .to_string(),
                )
            }
            _ => (
                MessageKind::SuperChat {
                    amount: "Super Sticker".to_string(),
                },
                display_message
                    .unwrap_or("sent a Super Sticker")
                    .to_string(),
            ),
        },

        MessageType::FanFundingEvent => (
            MessageKind::SuperChat {
                amount: "Fan Funding".to_string(),
            },
            display_message.unwrap_or("").to_string(),
        ),

        MessageType::NewSponsorEvent => {
            let level = match snippet.displayed_content.as_ref() {
                Some(DisplayedContent::NewSponsorDetails(d)) => d.member_level_name.as_deref(),
                _ => None,
            };
            let text = display_message
                .map(str::to_string)
                .unwrap_or_else(|| match level {
                    Some(level) => format!("became a member ({level})"),
                    None => "became a member".to_string(),
                });
            (MessageKind::Membership, text)
        }

        MessageType::MemberMilestoneChatEvent => {
            let text = match snippet.displayed_content.as_ref() {
                Some(DisplayedContent::MemberMilestoneChatDetails(d)) => {
                    let comment = d.user_comment.as_deref().filter(|c| !c.is_empty());
                    let months = d.member_month.unwrap_or(0);
                    match comment {
                        Some(comment) => format!("[{months} months] {comment}"),
                        None => format!("member for {months} months"),
                    }
                }
                _ => display_message.unwrap_or("member milestone").to_string(),
            };
            (MessageKind::Membership, text)
        }

        MessageType::MembershipGiftingEvent => {
            let text = match snippet.displayed_content.as_ref() {
                Some(DisplayedContent::MembershipGiftingDetails(d)) => {
                    let count = d.gift_memberships_count.unwrap_or(0);
                    format!("gifted {count} memberships")
                }
                _ => display_message.unwrap_or("gifted memberships").to_string(),
            };
            (MessageKind::Membership, text)
        }

        MessageType::GiftMembershipReceivedEvent => (
            MessageKind::Membership,
            display_message
                .unwrap_or("received a gifted membership")
                .to_string(),
        ),

        MessageType::MessageDeletedEvent => (
            MessageKind::System,
            display_message.unwrap_or("deleted a message").to_string(),
        ),

        MessageType::MessageRetractedEvent => (
            MessageKind::System,
            display_message.unwrap_or("retracted a message").to_string(),
        ),

        MessageType::UserBannedEvent => {
            let text = match snippet.displayed_content.as_ref() {
                Some(DisplayedContent::UserBannedDetails(d)) => {
                    let banned = d
                        .banned_user_details
                        .as_ref()
                        .and_then(|u| u.display_name.as_deref())
                        .unwrap_or("a user");
                    match d.ban_duration_seconds {
                        Some(seconds) if seconds > 0 => {
                            format!("timed out {banned} for {seconds}s")
                        }
                        _ => format!("banned {banned}"),
                    }
                }
                _ => display_message.unwrap_or("banned a user").to_string(),
            };
            (MessageKind::System, text)
        }

        MessageType::ChatEndedEvent => (
            MessageKind::System,
            display_message.unwrap_or("Live chat has ended").to_string(),
        ),

        MessageType::SponsorOnlyModeStartedEvent => (
            MessageKind::System,
            display_message
                .unwrap_or("Members-only mode enabled")
                .to_string(),
        ),

        MessageType::SponsorOnlyModeEndedEvent => (
            MessageKind::System,
            display_message
                .unwrap_or("Members-only mode disabled")
                .to_string(),
        ),

        // Tombstones are silent by definition; polls and unknown future types
        // have no sensible single-line rendering.
        MessageType::Tombstone | MessageType::PollEvent | MessageType::InvalidType => return None,
    };

    // Silent messages carry no display content and should not be rendered.
    if snippet.has_display_content == Some(false) && message.is_empty() {
        return None;
    }

    Some(ChatMessage {
        timestamp: format_timestamp(snippet.published_at.as_deref()),
        author,
        message,
        kind,
        avatar: None,
        avatar_url,
        is_member,
    })
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
    use crate::youtube_api_v3::{
        ChannelProfileDetails, LiveChatMemberMilestoneChatDetails,
        LiveChatMembershipGiftingDetails, LiveChatMessageAuthorDetails, LiveChatMessageSnippet,
        LiveChatSuperChatDetails, LiveChatTextMessageDetails, LiveChatUserBannedMessageDetails,
    };

    fn author(name: &str) -> LiveChatMessageAuthorDetails {
        LiveChatMessageAuthorDetails {
            display_name: Some(name.to_string()),
            profile_image_url: Some("https://yt3.ggpht.com/a.jpg".to_string()),
            is_chat_sponsor: Some(false),
            ..Default::default()
        }
    }

    fn message(kind: MessageType, snippet: LiveChatMessageSnippet, id: &str) -> LiveChatMessage {
        LiveChatMessage {
            id: Some(id.to_string()),
            snippet: Some(LiveChatMessageSnippet {
                r#type: Some(kind as i32),
                published_at: Some("2026-08-01T14:32:07.123Z".to_string()),
                ..snippet
            }),
            author_details: Some(author("emre")),
            ..Default::default()
        }
    }

    #[test]
    fn maps_a_plain_text_message() {
        let item = message(
            MessageType::TextMessageEvent,
            LiveChatMessageSnippet {
                displayed_content: Some(DisplayedContent::TextMessageDetails(
                    LiveChatTextMessageDetails {
                        message_text: Some("merhaba".to_string()),
                    },
                )),
                ..Default::default()
            },
            "id-1",
        );

        let mapped = map_message(&item).expect("text message should map");
        assert_eq!(mapped.message, "merhaba");
        assert_eq!(mapped.author, "emre");
        assert_eq!(mapped.timestamp, "14:32");
        assert_eq!(mapped.kind, MessageKind::Text);
        assert_eq!(
            mapped.avatar_url.as_deref(),
            Some("https://yt3.ggpht.com/a.jpg")
        );
    }

    // Regression: `published_at` used to be `.unwrap()`ed, which killed the
    // whole chat task when YouTube omitted it.
    #[test]
    fn missing_published_at_does_not_panic() {
        let mut item = message(
            MessageType::TextMessageEvent,
            LiveChatMessageSnippet {
                display_message: Some("hi".to_string()),
                ..Default::default()
            },
            "id-2",
        );
        item.snippet.as_mut().unwrap().published_at = None;

        let mapped = map_message(&item).expect("should still map");
        assert_eq!(mapped.timestamp, "--:--");
    }

    #[test]
    fn malformed_published_at_falls_back() {
        for ts in ["", "2026", "not-a-timestamp-at-all", "2026-08-01"] {
            let mut item = message(
                MessageType::TextMessageEvent,
                LiveChatMessageSnippet {
                    display_message: Some("hi".to_string()),
                    ..Default::default()
                },
                "id",
            );
            item.snippet.as_mut().unwrap().published_at = Some(ts.to_string());

            let mapped = map_message(&item).expect("should map");
            assert_eq!(mapped.timestamp, "--:--", "timestamp was {ts:?}");
        }
    }

    #[test]
    fn missing_author_details_falls_back() {
        let mut item = message(
            MessageType::TextMessageEvent,
            LiveChatMessageSnippet {
                display_message: Some("hi".to_string()),
                ..Default::default()
            },
            "id-3",
        );
        item.author_details = None;

        let mapped = map_message(&item).expect("should map");
        assert_eq!(mapped.author, "<unknown>");
        assert_eq!(mapped.avatar_url, None);
        assert!(!mapped.is_member);
    }

    #[test]
    fn item_without_snippet_is_skipped() {
        let item = LiveChatMessage {
            id: Some("id".to_string()),
            ..Default::default()
        };
        assert!(map_message(&item).is_none());
    }

    #[test]
    fn maps_super_chat_with_amount_and_comment() {
        let item = message(
            MessageType::SuperChatEvent,
            LiveChatMessageSnippet {
                displayed_content: Some(DisplayedContent::SuperChatDetails(
                    LiveChatSuperChatDetails {
                        amount_display_string: Some("₺100.00".to_string()),
                        user_comment: Some("iyi yayınlar".to_string()),
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
            "id-4",
        );

        let mapped = map_message(&item).expect("super chat should map");
        assert_eq!(
            mapped.kind,
            MessageKind::SuperChat {
                amount: "₺100.00".to_string()
            }
        );
        assert_eq!(mapped.message, "iyi yayınlar");
    }

    #[test]
    fn maps_membership_events() {
        let new_member = message(
            MessageType::NewSponsorEvent,
            LiveChatMessageSnippet::default(),
            "id-5",
        );
        let mapped = map_message(&new_member).expect("new sponsor should map");
        assert_eq!(mapped.kind, MessageKind::Membership);
        assert_eq!(mapped.message, "became a member");

        let milestone = message(
            MessageType::MemberMilestoneChatEvent,
            LiveChatMessageSnippet {
                displayed_content: Some(DisplayedContent::MemberMilestoneChatDetails(
                    LiveChatMemberMilestoneChatDetails {
                        member_month: Some(12),
                        user_comment: Some("bir yıl oldu".to_string()),
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
            "id-6",
        );
        let mapped = map_message(&milestone).expect("milestone should map");
        assert_eq!(mapped.message, "[12 months] bir yıl oldu");

        let gifting = message(
            MessageType::MembershipGiftingEvent,
            LiveChatMessageSnippet {
                displayed_content: Some(DisplayedContent::MembershipGiftingDetails(
                    LiveChatMembershipGiftingDetails {
                        gift_memberships_count: Some(5),
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
            "id-7",
        );
        let mapped = map_message(&gifting).expect("gifting should map");
        assert_eq!(mapped.message, "gifted 5 memberships");
        assert_eq!(mapped.kind, MessageKind::Membership);
    }

    #[test]
    fn maps_moderation_events() {
        let ban = message(
            MessageType::UserBannedEvent,
            LiveChatMessageSnippet {
                displayed_content: Some(DisplayedContent::UserBannedDetails(
                    LiveChatUserBannedMessageDetails {
                        banned_user_details: Some(ChannelProfileDetails {
                            display_name: Some("spammer".to_string()),
                            ..Default::default()
                        }),
                        ban_duration_seconds: Some(300),
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
            "id-8",
        );

        let mapped = map_message(&ban).expect("ban should map");
        assert_eq!(mapped.kind, MessageKind::System);
        assert_eq!(mapped.message, "timed out spammer for 300s");
    }

    #[test]
    fn chat_ended_is_detected() {
        let ended = message(
            MessageType::ChatEndedEvent,
            LiveChatMessageSnippet::default(),
            "id-9",
        );
        assert!(is_chat_ended(&ended));

        let mapped = map_message(&ended).expect("chat ended should render");
        assert_eq!(mapped.kind, MessageKind::System);

        let text = message(
            MessageType::TextMessageEvent,
            LiveChatMessageSnippet {
                display_message: Some("hi".to_string()),
                ..Default::default()
            },
            "id-10",
        );
        assert!(!is_chat_ended(&text));
    }

    #[test]
    fn tombstones_are_skipped() {
        let item = message(
            MessageType::Tombstone,
            LiveChatMessageSnippet::default(),
            "id-11",
        );
        assert!(map_message(&item).is_none());
    }

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
}
