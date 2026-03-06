use std::sync::Arc;

use ratatui::crossterm::event::KeyEvent;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Tick,
    Input(KeyEvent),
    Chat(ChatMessage),
    Status(StatusEvent),
    Error(String),
    StatsUpdate(StatsMessage),
    Quit,
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
    pub is_member: bool
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Text,
    Subscription,
}

#[derive(Debug, Clone)]
pub enum StatusEvent {
    Connecting,
    Connected,
    Disconnected,
}
