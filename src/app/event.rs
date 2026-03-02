use std::sync::Arc;
use ratatui::crossterm::event::KeyEvent;
use ratatui_image::protocol::Protocol;

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

#[derive(Clone)]
pub struct ChatMessage {
    pub timestamp: String,
    pub author: String,
    pub message: String,
    pub kind: MessageKind,
    pub avatar: Option<Arc<Protocol>>,
}

impl std::fmt::Debug for ChatMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatMessage")
            .field("timestamp", &self.timestamp)
            .field("author", &self.author)
            .field("message", &self.message)
            .field("has_avatar", &self.avatar.is_some())
            .field("kind", &self.kind)
            .finish()
    }
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
