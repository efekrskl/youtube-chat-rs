use crate::app::event::{ChatMessage, KittyAvatar, StatusEvent};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use std::collections::VecDeque;
use std::sync::Arc;

pub struct ScrollState {
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub visible_rows: usize,
    pub max_scroll_rows: usize,
}

pub struct Stats {
    pub viewer_count: u32
}

pub struct ConnectionState {
    pub status: StatusEvent,
    pub last_error: Option<String>,
}

pub struct AppState {
    pub title: String,
    pub messages: VecDeque<ChatMessage>,
    pub connection: ConnectionState,
    pub scroll_state: ScrollState,
    pub stats: Stats,
    pub dropped_messages: usize,
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

    /// Attach an avatar that finished downloading after its messages were
    /// already displayed.
    pub fn attach_avatar(&mut self, url: &str, avatar: Arc<KittyAvatar>) {
        for msg in self.messages.iter_mut() {
            if msg.avatar.is_none() && msg.avatar_url.as_deref() == Some(url) {
                msg.avatar = Some(avatar.clone());
            }
        }
    }

    pub fn note_dropped(&mut self, count: usize) {
        self.dropped_messages = self.dropped_messages.saturating_add(count);
    }

    fn scroll_up(&mut self, amount: usize) {
        self.scroll_state.scroll_offset = self.scroll_state.scroll_offset.saturating_add(amount);
    }

    fn scroll_down(&mut self, amount: usize) {
        self.scroll_state.scroll_offset = self.scroll_state.scroll_offset.saturating_sub(amount);
    }

    pub fn update_scroll_state(&mut self, visible_rows: usize, max_scroll_rows: usize) {
        self.scroll_state.visible_rows = visible_rows;
        self.scroll_state.max_scroll_rows = max_scroll_rows;
        self.scroll_state.scroll_offset = self
            .scroll_state
            .scroll_offset
            .min(self.scroll_state.max_scroll_rows);
    }

    pub fn update_stats(&mut self, viewer_count: u32) {
        self.stats.viewer_count = viewer_count;
    }

    pub fn update_status(&mut self, status: StatusEvent) {
        let connected = matches!(status, StatusEvent::Connected);
        self.connection.status = status;
        // Only a successful connection clears the previous failure; while we
        // are still retrying the user should keep seeing why.
        if connected {
            self.connection.last_error = None;
        }
    }

    pub fn set_error(&mut self, error: String) {
        // The status is owned by the producer now, which reports `Reconnecting`
        // while it retries; forcing `Disconnected` here hid that.
        self.connection.last_error = Some(error);
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
