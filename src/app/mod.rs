use crate::app::event::AppEvent;
use crate::app::state::{AppState, ScrollState, Stats};
use crate::app::ui::{draw_avatar_overlays, draw_with_overlays};
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
}

impl App {
    pub fn new(title: String, avatar_pixels: (u16, u16)) -> Self {
        Self {
            state: AppState {
                title,
                messages: Default::default(),
                scroll_state: ScrollState {
                    scroll_offset: 0,
                    auto_scroll: true,
                    visible_rows: 1,
                    max_scroll_rows: 0,
                },
                stats: Stats { viewer_count: 0 },
            },
            avatar_pixels,
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
        let max_scroll = crate::app::ui::max_scroll_for_viewport(&self.state, chat_width, visible_rows);
        self.state.update_scroll_state(visible_rows, max_scroll);
        let mut overlays = Vec::new();

        terminal.draw(|f| {
            overlays = draw_with_overlays(f, &self.state);
        })?;

        draw_avatar_overlays(&overlays, self.avatar_pixels)?;

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
