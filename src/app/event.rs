use std::sync::Arc;

use ratatui::crossterm::event::KeyEvent;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Input(KeyEvent),
    Chat(ChatMessage),
    Status(StatusEvent),
    Error(String),
    StatsUpdate(StatsMessage),
}

#[derive(Debug, Clone)]
pub struct StatsMessage {
    pub viewer_count: u32,
}

#[derive(Debug, Clone)]
pub struct KittyAvatar {
    pub id: u32,
    pub cols: u16,
    pub width: u32,
    pub height: u32,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub timestamp: String,
    pub author: String,
    pub message: String,
    pub kind: MessageKind,
    pub avatar: Option<Arc<KittyAvatar>>,
    /// Where the author's avatar can be fetched from, if they have one.
    pub avatar_url: Option<String>,
    pub is_member: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageKind {
    Text,
    /// Paid message; carries the formatted amount as YouTube reports it.
    SuperChat {
        amount: String,
    },
    /// New member, milestone, gifted membership.
    Membership,
    /// Moderation and lifecycle notices (deleted message, ban, chat ended).
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusEvent {
    Connecting,
    Connected,
    Reconnecting { attempt: u32 },
    Disconnected,
}
