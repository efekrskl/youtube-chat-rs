use crate::app::event::ChatMessage;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use std::collections::VecDeque;

pub struct ScrollState {
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub visible_rows: usize,
    pub max_scroll_rows: usize,
}
pub struct AppState {
    pub title: String,
    pub messages: VecDeque<ChatMessage>,
    // todo: pub status: String,
    // todo: viewer_count: Option<u16>
    pub scroll_state: ScrollState,
}

const MAX_MESSAGES: usize = 500;

impl AppState {
    pub fn push_message(&mut self, msg: ChatMessage) {
        if !self.scroll_state.auto_scroll {
            self.scroll_state.scroll_offset = self.scroll_state.scroll_offset.saturating_add(1);
        }
        self.messages.push_back(msg);
        if self.scroll_state.auto_scroll {
            self.scroll_state.scroll_offset = 0;
        }

        while self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
            if self.scroll_state.scroll_offset > 0 {
                self.scroll_state.scroll_offset = self.scroll_state.scroll_offset.saturating_sub(1);
            }
        }
    }

    fn scroll_up(&mut self, amount: usize) {
        self.scroll_state.scroll_offset = self.scroll_state.scroll_offset.saturating_add(amount);
    }

    fn scroll_down(&mut self, amount: usize) {
        self.scroll_state.scroll_offset = self.scroll_state.scroll_offset.saturating_sub(amount);
    }

    pub fn update_scroll_metrics(&mut self, visible_rows: usize, max_scroll_rows: usize) {
        self.scroll_state.visible_rows = visible_rows;
        self.scroll_state.max_scroll_rows = max_scroll_rows;
        self.scroll_state.scroll_offset = self
            .scroll_state
            .scroll_offset
            .min(self.scroll_state.max_scroll_rows);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        let page = self.scroll_state.visible_rows.max(1);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return true,
            KeyCode::Up => {
                self.scroll_up(1);
                self.scroll_state.auto_scroll = false;
            }
            KeyCode::Down => {
                self.scroll_down(1);
                self.scroll_state.auto_scroll = self.scroll_state.scroll_offset == 0;
            }
            KeyCode::PageUp => {
                self.scroll_up(page);
                self.scroll_state.auto_scroll = false;
            }
            KeyCode::PageDown => {
                self.scroll_down(page);
                self.scroll_state.auto_scroll = self.scroll_state.scroll_offset == 0;
            }
            KeyCode::Home => {
                self.scroll_state.scroll_offset = self.scroll_state.max_scroll_rows;
                self.scroll_state.auto_scroll = false;
            }
            KeyCode::End => {
                self.scroll_state.scroll_offset = 0;
                self.scroll_state.auto_scroll = true;
            }
            _ => {}
        }

        self.scroll_state.scroll_offset = self
            .scroll_state
            .scroll_offset
            .min(self.scroll_state.max_scroll_rows);

        false
    }
}
