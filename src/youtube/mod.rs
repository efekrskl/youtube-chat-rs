use tokio::sync::mpsc;
use crate::app::event::AppEvent;
use crate::youtube::api::YoutubeService;

pub mod api;
pub mod auth;
pub mod models;

pub fn spawn_youtube_chat_task(
    yt: YoutubeService,
    live_chat_id: String,
    tx: mpsc::Sender<AppEvent>
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // todo let _ = tx.send(AppEvent::Status(StatusEvent::Connecting))
        
        match yt.stream_chat(&live_chat_id, tx.clone()).await {
            Ok(_) => {},
            Err(e) => {
                // todo: AppEvent::Error
            }
        }
    })
}