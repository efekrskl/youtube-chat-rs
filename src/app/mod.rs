use std::io::Stdout;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use crate::app::event::AppEvent;
use crate::app::state::AppState;
use crate::app::ui::draw;

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
                scroll_offset: 0,
                auto_scroll: true
            },
        }
    }

    pub fn on_event(&mut self, event: AppEvent, visible_rows: usize) -> bool {
        match event {
            AppEvent::Chat(msg) => self.state.push_message(msg),
            AppEvent::Input(key) => {
                if self.state.handle_key(key, visible_rows) {
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

            let Some(ev) = rx.recv().await else { break };
            if self.on_event(ev, visible_rows) {
                break;
            }
        }
        Ok(())
    }
}
