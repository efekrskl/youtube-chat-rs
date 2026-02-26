use crate::app::event::AppEvent;
use ratatui::crossterm::event;
use ratatui::crossterm::event::{Event, KeyEventKind};
use std::time::Duration;
use tokio::sync::mpsc;

pub fn spawn_input_task(tx: mpsc::Sender<AppEvent>) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        loop {
            if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                if let Ok(Event::Key(key)) = event::read() {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    let _ = tx.blocking_send(AppEvent::Input(key));
                }
            }
        }
    })
}
