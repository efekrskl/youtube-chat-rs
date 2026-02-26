use crate::app::event::ChatMessage;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::VecDeque;

pub struct AppState {
    pub title: String,
    pub messages: VecDeque<ChatMessage>,
    // todo: pub status: String,
    // todo: viewer_count: Option<u16>
    pub scroll_offset: usize,
    pub auto_scroll: bool,
}

const MAX_MESSAGES: usize = 500;

impl AppState {
    pub fn push_message(&mut self, msg: ChatMessage) {
        if !self.auto_scroll {
            self.scroll_offset = self.scroll_offset.saturating_add(1);
        }
        self.messages.push_back(msg);
        if self.auto_scroll {
            self.scroll_offset = 0;
        }

        while self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
            if self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
        }
    }

    fn max_scroll(&self, visible_rows: usize) -> usize {
        self.messages.len().saturating_sub(visible_rows)
    }

    fn scroll_up(&mut self, amount: usize, visible_rows: usize) {
        let max_scroll = self.max_scroll(visible_rows);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }

    fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn handle_key(&mut self, key: KeyEvent, visible_rows: usize) -> bool {
        let page = visible_rows.max(1);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return true,
            KeyCode::Up => {
                self.scroll_up(1, visible_rows);
                self.auto_scroll = false;
            }
            KeyCode::Down => {
                self.scroll_down(1);
                self.auto_scroll = self.scroll_offset == 0;
            }
            KeyCode::PageUp => {
                self.scroll_up(page, visible_rows);
                self.auto_scroll = false;
            }
            KeyCode::PageDown => {
                self.scroll_down(page);
                self.auto_scroll = self.scroll_offset == 0;
            }
            KeyCode::Home => {
                self.scroll_offset = self.max_scroll(visible_rows);
                self.auto_scroll = false;
            }
            KeyCode::End => {
                self.scroll_offset = 0;
                self.auto_scroll = true;
            }
            _ => {}
        }

        false
    }
}
