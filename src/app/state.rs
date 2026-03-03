use crate::app::event::ChatMessage;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use std::collections::VecDeque;

pub struct ScrollState {
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub visible_rows: usize,
    pub max_scroll_rows: usize,
}

pub struct Stats {
    pub viewer_count: u32
}

pub struct CachedMessageLayout {
    pub body_lines: Vec<String>,
}

pub struct AppState {
    pub title: String,
    pub messages: VecDeque<ChatMessage>,
    pub layouts: VecDeque<CachedMessageLayout>,
    pub layout_width: Option<usize>,
    pub total_rows: usize,
    // todo: pub status: String,
    pub scroll_state: ScrollState,
    pub stats: Stats
}

const MAX_MESSAGES: usize = 500;
const AVATAR_WIDTH: usize = 2;
const AVATAR_GAP: usize = 1;

impl AppState {
    fn build_layout(msg: &ChatMessage, chat_width: usize) -> CachedMessageLayout {
        let body_lines = if msg.kind == crate::app::event::MessageKind::Text {
            let prefix = format!("[{}] {}: ", msg.timestamp, msg.author);
            let prefix_len = AVATAR_WIDTH + AVATAR_GAP + prefix.chars().count();
            let body_width = chat_width.saturating_sub(prefix_len).max(1);
            let wrapped = textwrap::wrap(&msg.message, body_width);

            if wrapped.is_empty() {
                vec![msg.message.clone()]
            } else {
                wrapped.into_iter().map(|part| part.into_owned()).collect()
            }
        } else {
            vec![msg.message.clone()]
        };

        CachedMessageLayout { body_lines }
    }

    pub fn ensure_layout_cache(&mut self, chat_width: usize) {
        let needs_rebuild = self.layout_width != Some(chat_width)
            || self.layouts.len() != self.messages.len();

        if !needs_rebuild {
            return;
        }

        self.layouts.clear();
        self.total_rows = 0;

        for msg in &self.messages {
            let layout = Self::build_layout(msg, chat_width);
            self.total_rows += layout.body_lines.len().max(1);
            self.layouts.push_back(layout);
        }

        self.layout_width = Some(chat_width);
    }

    pub fn push_message(&mut self, msg: ChatMessage) {
        let mut rows_added = 1;
        let mut cached_layout = None;

        if let Some(chat_width) = self.layout_width {
            let layout = Self::build_layout(&msg, chat_width);
            rows_added = layout.body_lines.len().max(1);
            cached_layout = Some(layout);
        }

        if !self.scroll_state.auto_scroll {
            self.scroll_state.scroll_offset = self
                .scroll_state
                .scroll_offset
                .saturating_add(rows_added);
        }
        self.messages.push_back(msg);
        self.layouts.push_back(cached_layout.unwrap_or(CachedMessageLayout {
            body_lines: Vec::new(),
        }));
        self.total_rows = self.total_rows.saturating_add(rows_added);

        if self.scroll_state.auto_scroll {
            self.scroll_state.scroll_offset = 0;
        }

        while self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
            if let Some(layout) = self.layouts.pop_front() {
                self.total_rows = self
                    .total_rows
                    .saturating_sub(layout.body_lines.len().max(1));
            }
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
