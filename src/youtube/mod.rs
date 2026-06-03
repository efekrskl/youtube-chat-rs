use crate::app::event::{AppEvent, StatusEvent};
use crate::youtube::api::YoutubeService;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub mod api;
pub mod auth;
pub mod models;

const MAX_RECONNECT_ATTEMPTS: usize = 5;
const HEALTHY_SESSION_THRESHOLD: Duration = Duration::from_secs(30);

pub fn spawn_youtube_chat_task(
    yt: YoutubeService,
    live_chat_id: String,
    tx: mpsc::Sender<AppEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut reconnect_attempts = 0;

        loop {
            let _ = tx.send(AppEvent::Status(StatusEvent::Connecting)).await;

            let started_at = Instant::now();
            let result = yt.stream_chat(&live_chat_id, tx.clone()).await;
            if started_at.elapsed() >= HEALTHY_SESSION_THRESHOLD {
                reconnect_attempts = 0;
            }

            match result {
                Ok(_) => {
                    let _ = tx
                        .send(AppEvent::Error("chat stream ended".to_string()))
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(AppEvent::Error(format!("chat disconnected: {e}")))
                        .await;
                }
            }

            if tx.is_closed() || reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
                break;
            }

            tokio::time::sleep(reconnect_delay(reconnect_attempts)).await;
            reconnect_attempts += 1;
        }
    })
}

fn reconnect_delay(attempt: usize) -> Duration {
    Duration::from_secs(1 << attempt.min(4))
}
