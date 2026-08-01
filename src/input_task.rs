use std::time::Duration;

use log::debug;
use ratatui::crossterm::event;
use ratatui::crossterm::event::{Event, KeyEventKind};
use tokio::sync::mpsc;

use crate::app::event::AppEvent;

/// Stop polling after this many consecutive failures rather than spinning on a
/// terminal that will never produce another event.
const MAX_CONSECUTIVE_POLL_ERRORS: u32 = 100;

pub fn spawn_input_task(tx: mpsc::Sender<AppEvent>) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        let mut consecutive_errors = 0u32;

        loop {
            if tx.is_closed() {
                break;
            }

            match event::poll(Duration::from_millis(50)) {
                Ok(true) => {
                    consecutive_errors = 0;
                    match event::read() {
                        Ok(Event::Key(key)) => {
                            if key.kind != KeyEventKind::Press {
                                continue;
                            }
                            if tx.blocking_send(AppEvent::Input(key)).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            debug!("terminal read failed: {e}");
                            consecutive_errors += 1;
                        }
                    }
                }
                Ok(false) => consecutive_errors = 0,
                Err(e) => {
                    // A failing poll returns immediately, so without this the
                    // loop would burn a core.
                    debug!("terminal poll failed: {e}");
                    consecutive_errors += 1;
                    std::thread::sleep(Duration::from_millis(50));
                }
            }

            if consecutive_errors >= MAX_CONSECUTIVE_POLL_ERRORS {
                debug!("giving up on terminal input after repeated failures");
                break;
            }
        }
    })
}
