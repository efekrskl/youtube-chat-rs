use std::collections::HashSet;
use std::io::{Stdout, Write, stdout};
use std::time::{Duration, Instant};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use crate::app::event::{AppEvent, KittyAvatar};
use crate::app::state::AppState;
use crate::app::ui::draw;

pub mod event;
pub mod state;
pub mod ui;

/// Cap redraws so a fast-moving chat cannot spend the whole runtime painting.
const MIN_REDRAW_INTERVAL: Duration = Duration::from_millis(33);

struct Graphics {
    kitty_supported: bool,
    loaded_avatar_ids: HashSet<u32>,
}

pub struct App {
    pub state: AppState,
    graphics: Graphics,
    last_draw: Option<Instant>,
}

impl App {
    pub fn new(title: String) -> Self {
        Self {
            state: AppState::new(title),
            graphics: Graphics {
                kitty_supported: std::env::var("TERM")
                    .map(|term| term == "xterm-kitty")
                    .unwrap_or(false),
                loaded_avatar_ids: HashSet::new(),
            },
            last_draw: None,
        }
    }

    /// Returns true when the app should quit.
    pub fn on_event(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::Chat(mut msg) => {
                if self.graphics.kitty_supported {
                    if let Some(avatar) = msg.avatar.as_ref() {
                        let _ = prepare_kitty_avatar(avatar, &mut self.graphics.loaded_avatar_ids);
                    }
                } else {
                    msg.avatar = None;
                    msg.avatar_url = None;
                }
                self.state.push_message(msg);
            }
            AppEvent::AvatarReady { url, avatar } => {
                if self.graphics.kitty_supported {
                    let _ = prepare_kitty_avatar(&avatar, &mut self.graphics.loaded_avatar_ids);
                    self.state.attach_avatar(&url, avatar);
                }
            }
            AppEvent::Input(key) => {
                if self.state.handle_key(key) {
                    return true;
                }
            }
            AppEvent::StatsUpdate(stats) => self.state.update_stats(stats.viewer_count),
            AppEvent::Status(status) => self.state.update_status(status),
            AppEvent::Error(error) => self.state.set_error(error),
            AppEvent::Dropped(count) => self.state.note_dropped(count),
        }

        false
    }

    fn render(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
        let mut metrics = None;
        terminal.draw(|f| metrics = Some(draw(f, &self.state)))?;

        // `draw` already measured the layout; recomputing it here is what used
        // to make the viewport height disagree with what was rendered.
        if let Some(metrics) = metrics {
            self.state
                .update_scroll_state(metrics.visible_rows, metrics.max_scroll_rows);
        }
        self.last_draw = Some(Instant::now());

        Ok(())
    }

    fn should_redraw(&self) -> bool {
        self.last_draw
            .is_none_or(|last| last.elapsed() >= MIN_REDRAW_INTERVAL)
    }

    pub async fn run(
        mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        mut rx: mpsc::Receiver<AppEvent>,
    ) -> anyhow::Result<()> {
        self.render(terminal)?;

        loop {
            let Some(ev) = rx.recv().await else { break };
            if self.on_event(ev) {
                break;
            }

            // Drain whatever else is queued so a burst costs one redraw.
            while let Ok(ev) = rx.try_recv() {
                if self.on_event(ev) {
                    return Ok(());
                }
            }

            if self.should_redraw() {
                self.render(terminal)?;
            } else {
                // Let the pending events settle, then paint once.
                tokio::time::sleep(MIN_REDRAW_INTERVAL).await;
                while let Ok(ev) = rx.try_recv() {
                    if self.on_event(ev) {
                        return Ok(());
                    }
                }
                self.render(terminal)?;
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
        // `t=f` (regular file), not `t=t`: the avatars now live in a bounded
        // cache under the app directory that we manage, not in a temp dir that
        // kitty is free to delete out from under us.
        write!(
            out,
            "\x1b_Ga=T,U=1,t=f,f=32,s={},v={},i={},c={},r=1,q=2;{}\x1b\\",
            avatar.width,
            avatar.height,
            avatar.id,
            avatar.cols,
            STANDARD.encode(avatar.path.as_bytes()),
        )?;
        out.flush()?;
    }

    Ok(())
}
