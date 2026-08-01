use std::time::Duration;

use log::debug;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval};

use crate::app::event::{AppEvent, StatsMessage};
use crate::youtube::api::YoutubeService;

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

            if tx.is_closed() {
                break;
            }

            match yt.get_viewer_count_by_video_id(&live_video_id).await {
                Ok(Some(viewer_count)) => {
                    debug!("fetched viewer count as {viewer_count}");
                    if tx
                        .send(AppEvent::StatsUpdate(StatsMessage { viewer_count }))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                // The stream may simply not report a viewer count; that is not
                // an error and must not be rendered as "0 viewers".
                Ok(None) => debug!("no concurrent viewer count reported"),
                // Don't surface this in the status bar: it would stomp on chat
                // connection errors, which matter far more.
                Err(e) => debug!("viewer count fetch failed: {e}"),
            }
        }
    })
}
