use ratatui::crossterm::event::KeyEvent;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Tick,
    Input(KeyEvent),
    Chat(ChatMessage),
    Status(StatusEvent),
    Error(String),
    Quit,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub timestamp: String,
    pub author: String,
    pub message: String,
    pub kind: MessageKind,
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