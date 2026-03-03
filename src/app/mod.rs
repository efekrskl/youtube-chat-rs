use crate::app::event::AppEvent;
use crate::app::state::{AppState, ScrollState, Stats};
use crate::app::ui::{AvatarOverlay, draw_avatar_overlays, draw_with_overlays};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::Stdout;
use tokio::sync::mpsc;

pub mod event;
pub mod state;
mod ui;

pub struct App {
    pub state: AppState,
    avatar_pixels: (u16, u16),
    last_overlays: Vec<AvatarOverlay>,
}

impl App {
    pub fn new(title: String, avatar_pixels: (u16, u16)) -> Self {
        Self {
            state: AppState {
                title,
                messages: Default::default(),
                layouts: Default::default(),
                layout_width: None,
                total_rows: 0,
                scroll_state: ScrollState {
                    scroll_offset: 0,
                    auto_scroll: true,
                    visible_rows: 1,
                    max_scroll_rows: 0,
                },
                stats: Stats { viewer_count: 0 },
            },
            avatar_pixels,
            last_overlays: Vec::new(),
        }
    }

    pub fn on_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::Chat(msg) => self.state.push_message(msg),
            AppEvent::Input(key) => {
                if self.state.handle_key(key) {
                    return true;
                }
            }
            AppEvent::StatsUpdate(stats) => self.state.update_stats(stats.viewer_count),
            _ => {
                // todo
            }
        }

        false
    }

    async fn handle_tui(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> anyhow::Result<()> {
        let size = terminal.size()?;
        let visible_rows = size.height.saturating_sub(3) as usize;
        let chat_width = size.width.saturating_sub(2) as usize;
        self.state.ensure_layout_cache(chat_width);
        let max_scroll = self.state.total_rows.saturating_sub(visible_rows);
        self.state.update_scroll_state(visible_rows, max_scroll);
        let mut overlays = Vec::new();

        terminal.draw(|f| {
            overlays = draw_with_overlays(f, &self.state);
        })?;

        if overlays != self.last_overlays {
            draw_avatar_overlays(&overlays, self.avatar_pixels)?;
            self.last_overlays = overlays;
        }

        Ok(())
    }

    pub async fn run(
        mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        mut rx: mpsc::Receiver<AppEvent>,
    ) -> anyhow::Result<()> {
        self.handle_tui(terminal).await?;

        loop {
            let Some(ev) = rx.recv().await else { break };
            if self.on_event(ev) {
                break;
            }

            while let Ok(ev) = rx.try_recv() {
                if self.on_event(ev) {
                    return Ok(());
                }
            }

            self.handle_tui(terminal).await?;
        }
        Ok(())
    }
}
