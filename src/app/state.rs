use crate::app::event::ChatMessage;
use std::collections::VecDeque;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub struct AppState {
    pub title: String,
    pub messages: VecDeque<ChatMessage>,
    pub status: String, // todo: viewer_count: Option<u16>
}

const MAX_MESSAGES: usize = 500;

impl AppState {
    pub fn push_message(&mut self, msg: ChatMessage) {
        self.messages.push_back(msg);

        while self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
        }
    }

    pub fn handle_key(&self, key: KeyEvent) -> bool {
        let is_ctrl_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        let is_quit = matches!(key.code, KeyCode::Esc | KeyCode::Char('q'));

        is_ctrl_c || is_quit
    }
}
