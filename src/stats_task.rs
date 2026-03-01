use crate::app::event::{AppEvent, StatsMessage};
use crate::youtube::api::YoutubeService;
use std::time::Duration;
use log::debug;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval};

pub fn spawn_stats_task(
    live_video_id: String,
    yt: YoutubeService,
    tx: mpsc::Sender<AppEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn(async move {
        let mut interval = interval(Duration::from_secs(10));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            match yt.get_viewer_count_by_video_id(&live_video_id).await {
                Ok(viewer_count) => {
                    debug!("fetched viewer count as {}", viewer_count);
                    if tx
                        .send(AppEvent::StatsUpdate(StatsMessage {
                            viewer_count: viewer_count.parse::<u32>().unwrap_or(0),
                        }))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                _ => {
                    //todo: send err message
                }
            }
        }
    })
}
