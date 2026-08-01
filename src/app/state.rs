use std::collections::VecDeque;
use std::sync::Arc;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::event::{ChatMessage, KittyAvatar, StatusEvent};

pub struct ScrollState {
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub visible_rows: usize,
    pub max_scroll_rows: usize,
}

pub struct Stats {
    pub viewer_count: u32,
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

pub const MAX_MESSAGES: usize = 500;

impl AppState {
    pub fn new(title: String) -> Self {
        Self {
            title,
            messages: VecDeque::with_capacity(MAX_MESSAGES),
            connection: ConnectionState {
                status: StatusEvent::Connecting,
                last_error: None,
            },
            scroll_state: ScrollState {
                scroll_offset: 0,
                auto_scroll: true,
                visible_rows: 1,
                max_scroll_rows: 0,
            },
            stats: Stats { viewer_count: 0 },
            dropped_messages: 0,
        }
    }

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
        self.connection.last_error = Some(error);
    }

    /// Returns true when the app should quit.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        let page = self.scroll_state.visible_rows.max(1);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::event::MessageKind;

    fn msg(text: &str) -> ChatMessage {
        ChatMessage {
            timestamp: "12:00".to_string(),
            author: "author".to_string(),
            message: text.to_string(),
            kind: MessageKind::Text,
            avatar: None,
            avatar_url: None,
            is_member: false,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn ring_buffer_caps_message_count() {
        let mut state = AppState::new("t".into());
        for i in 0..(MAX_MESSAGES + 50) {
            state.push_message(msg(&i.to_string()));
        }
        assert_eq!(state.messages.len(), MAX_MESSAGES);
        // The oldest messages were evicted, the newest retained.
        assert_eq!(state.messages.back().unwrap().message, "549");
        assert_eq!(state.messages.front().unwrap().message, "50");
    }

    #[test]
    fn auto_scroll_pins_to_the_bottom() {
        let mut state = AppState::new("t".into());
        state.update_scroll_state(10, 100);
        for _ in 0..20 {
            state.push_message(msg("x"));
        }
        assert_eq!(state.scroll_state.scroll_offset, 0);
        assert!(state.scroll_state.auto_scroll);
    }

    #[test]
    fn manual_scroll_keeps_position_as_messages_arrive() {
        let mut state = AppState::new("t".into());
        state.update_scroll_state(10, 100);
        state.handle_key(key(KeyCode::Up));
        assert!(!state.scroll_state.auto_scroll);
        let before = state.scroll_state.scroll_offset;

        state.push_message(msg("new"));
        // The viewport stays anchored on the same message.
        assert_eq!(state.scroll_state.scroll_offset, before + 1);
    }

    // Once the ring buffer is saturated, eviction must cancel out the offset
    // bump, otherwise a long session scrolled up would drift forever.
    #[test]
    fn eviction_while_scrolled_stops_the_offset_from_growing() {
        let mut state = AppState::new("t".into());
        state.update_scroll_state(10, MAX_MESSAGES);
        state.handle_key(key(KeyCode::Up));

        for i in 0..(MAX_MESSAGES * 2) {
            state.push_message(msg(&i.to_string()));
        }
        let saturated = state.scroll_state.scroll_offset;

        for i in 0..(MAX_MESSAGES * 2) {
            state.push_message(msg(&i.to_string()));
        }
        assert_eq!(state.scroll_state.scroll_offset, saturated);

        // And the next frame clamps it into the renderable range regardless.
        state.update_scroll_state(10, 42);
        assert_eq!(state.scroll_state.scroll_offset, 42);
    }

    #[test]
    fn end_key_resumes_following() {
        let mut state = AppState::new("t".into());
        state.update_scroll_state(10, 100);
        state.handle_key(key(KeyCode::PageUp));
        assert!(!state.scroll_state.auto_scroll);

        state.handle_key(key(KeyCode::End));
        assert!(state.scroll_state.auto_scroll);
        assert_eq!(state.scroll_state.scroll_offset, 0);
    }

    #[test]
    fn scrolling_down_to_the_bottom_re_enables_following() {
        let mut state = AppState::new("t".into());
        state.update_scroll_state(10, 100);
        state.handle_key(key(KeyCode::Up));
        state.handle_key(key(KeyCode::Down));
        assert!(state.scroll_state.auto_scroll);
    }

    #[test]
    fn home_key_jumps_to_the_oldest_message() {
        let mut state = AppState::new("t".into());
        state.update_scroll_state(10, 42);
        state.handle_key(key(KeyCode::Home));
        assert_eq!(state.scroll_state.scroll_offset, 42);
        assert!(!state.scroll_state.auto_scroll);
    }

    #[test]
    fn offset_is_clamped_to_the_max() {
        let mut state = AppState::new("t".into());
        state.update_scroll_state(10, 5);
        for _ in 0..50 {
            state.handle_key(key(KeyCode::Up));
        }
        assert_eq!(state.scroll_state.scroll_offset, 5);
    }

    #[test]
    fn quit_keys_are_recognised() {
        let mut state = AppState::new("t".into());
        assert!(state.handle_key(key(KeyCode::Char('q'))));
        assert!(state.handle_key(key(KeyCode::Esc)));
        assert!(state.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)));
        assert!(!state.handle_key(key(KeyCode::Char('x'))));
    }

    #[test]
    fn avatars_arriving_late_are_attached_to_existing_messages() {
        let mut state = AppState::new("t".into());
        let mut m = msg("hi");
        m.avatar_url = Some("https://yt3.ggpht.com/a".to_string());
        state.push_message(m);
        state.push_message(msg("other"));

        let avatar = Arc::new(KittyAvatar {
            id: 7,
            cols: 2,
            width: 32,
            height: 32,
            path: "/tmp/a.rgba".to_string(),
        });
        state.attach_avatar("https://yt3.ggpht.com/a", avatar);

        assert!(state.messages[0].avatar.is_some());
        assert!(state.messages[1].avatar.is_none());
    }

    #[test]
    fn errors_persist_until_a_successful_connection() {
        let mut state = AppState::new("t".into());
        state.set_error("boom".into());
        // Still retrying: keep showing why.
        state.update_status(StatusEvent::Reconnecting { attempt: 2 });
        assert_eq!(state.connection.last_error.as_deref(), Some("boom"));

        state.update_status(StatusEvent::Connected);
        assert_eq!(state.connection.last_error, None);
    }
}
