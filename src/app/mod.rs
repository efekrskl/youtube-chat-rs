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
                status: "Starting...".to_string(),
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

            let Some(ev) = rx.recv().await else { break };
            if self.on_event(ev) {
                break;
            }
        }
        Ok(())
    }
}
