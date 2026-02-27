use std::io::Stdout;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use crate::app::event::AppEvent;
use crate::app::state::{AppState, ScrollState};
use crate::app::ui::{draw, max_scroll_for_viewport};

pub mod event;
pub mod state;
mod ui;

pub struct App {
    pub state: AppState,
}

impl App {
    pub fn new(title: String) -> Self {
        Self {
            state: AppState {
                title,
                messages: Default::default(),
                scroll_state: ScrollState {
                    scroll_offset: 0,
                    auto_scroll: true,
                    visible_rows: 1,
                    max_scroll_rows: 0,
                }
            },
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
            _ => {
                // todo
            }
        }

        false
    }

    pub async fn run(
        mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        mut rx: mpsc::Receiver<AppEvent>,
    ) -> anyhow::Result<()> {
        loop {
            terminal.draw(|f| draw(f, &self.state))?;
            let size = terminal.size()?;
            let visible_rows = size.height.saturating_sub(3) as usize;
            let chat_width = size.width.saturating_sub(2) as usize;
            let max_scroll = max_scroll_for_viewport(&self.state, chat_width, visible_rows);
            self.state.update_scroll_state(visible_rows, max_scroll);

            let Some(ev) = rx.recv().await else { break };
            if self.on_event(ev) {
                break;
            }
        }
        Ok(())
    }
}
