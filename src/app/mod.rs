use crate::app::event::{AppEvent, KittyAvatar};
use crate::app::state::{AppState, ScrollState, Stats};
use crate::app::ui::{draw, max_scroll_for_viewport};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::collections::HashSet;
use std::io::{Stdout, Write, stdout};
use tokio::sync::mpsc;

pub mod event;
pub mod state;
mod ui;

struct Graphics {
    kitty_supported: bool,
    loaded_avatar_ids: HashSet<u32>,
}
pub struct App {
    pub state: AppState,
    graphics: Graphics,
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
                },
                stats: Stats { viewer_count: 0 },
            },
            graphics: Graphics {
                kitty_supported: std::env::var("TERM")
                    .map(|term| matches!(term.as_str(), "xterm-kitty"))
                    .unwrap_or(false),
                loaded_avatar_ids: HashSet::new(),
            },
        }
    }

    pub fn on_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::Chat(mut msg) => {
                if self.graphics.kitty_supported {
                    if let Some(avatar) = msg.avatar.as_ref() {
                        let _ = prepare_kitty_avatar(avatar, &mut self.graphics.loaded_avatar_ids);
                    }
                } else {
                    msg.avatar = None;
                }
                self.state.push_message(msg)
            }
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
        terminal.draw(|f| draw(f, &self.state))?;
        let size = terminal.size()?;
        let visible_rows = size.height.saturating_sub(3) as usize;
        let chat_width = size.width.saturating_sub(2) as usize;
        let max_scroll = max_scroll_for_viewport(&self.state, chat_width, visible_rows);
        self.state.update_scroll_state(visible_rows, max_scroll);

        Ok(())
    }

    pub async fn run(
        mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        mut rx: mpsc::Receiver<AppEvent>,
    ) -> anyhow::Result<()> {
        loop {
            self.handle_tui(terminal).await?;

            let Some(ev) = rx.recv().await else { break };
            if self.on_event(ev) {
                break;
            }
        }
        Ok(())
    }
}

fn prepare_kitty_avatar(
    avatar: &KittyAvatar,
    loaded_avatar_ids: &mut HashSet<u32>,
) -> anyhow::Result<()> {
    let mut out = stdout();

    if loaded_avatar_ids.insert(avatar.id) {
        write!(
            out,
            "\x1b_Ga=T,U=1,t=t,f=32,s={},v={},i={},c={},r=1,q=2;{}\x1b\\",
            avatar.width,
            avatar.height,
            avatar.id,
            avatar.cols,
            STANDARD.encode(avatar.path.as_bytes()),
        )?;
    }

    out.flush()?;
    Ok(())
}
