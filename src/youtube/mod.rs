use std::time::{Duration, Instant};

use log::{debug, warn};
use tokio::sync::mpsc;
use tonic::transport::Channel;

use crate::app::event::{AppEvent, StatusEvent};
use crate::youtube::api::{StreamOutcome, YoutubeService};
use crate::youtube::error::{Recovery, YoutubeError};
use crate::youtube::message::MessageDedup;

pub mod api;
pub mod auth;
pub mod avatar;
pub mod error;
pub mod message;
pub mod models;

const BASE_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);
/// After this many consecutive failures, assume the broadcast may have
/// restarted under a new live chat id and re-resolve it.
const RE_RESOLVE_AFTER_ATTEMPTS: u32 = 3;
/// Floor on how often we may open a new `StreamList` subscription.
const MIN_SUBSCRIPTION_INTERVAL: Duration = Duration::from_millis(500);

/// Exponential backoff with a cap and deterministic jitter.
///
/// Jitter is derived from the attempt number rather than a RNG so the behaviour
/// stays testable; the goal is only to avoid every client in a fleet retrying
/// on the same tick.
pub fn reconnect_delay(attempt: u32, jitter_seed: u64) -> Duration {
    let exponent = attempt.min(5);
    let base = BASE_RECONNECT_DELAY
        .saturating_mul(1u32 << exponent)
        .min(MAX_RECONNECT_DELAY);

    // Up to 25% extra, never longer than the cap.
    let jitter_ms = (jitter_seed % 250) * base.as_millis() as u64 / 1000;
    base.saturating_add(Duration::from_millis(jitter_ms))
        .min(MAX_RECONNECT_DELAY)
}

pub fn spawn_youtube_chat_task(
    yt: YoutubeService,
    video_id: String,
    live_chat_id: String,
    tx: mpsc::Sender<AppEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut session = ChatSession {
            yt,
            video_id,
            live_chat_id,
            tx,
            page_token: None,
            dedup: MessageDedup::default(),
            channel: None,
            attempt: 0,
        };

        session.run().await;
    })
}

struct ChatSession {
    yt: YoutubeService,
    video_id: String,
    live_chat_id: String,
    tx: mpsc::Sender<AppEvent>,
    /// Kept across reconnects so we resume exactly where we left off; resetting
    /// it used to lose every message sent during the outage and replay the
    /// backlog on top.
    page_token: Option<String>,
    dedup: MessageDedup,
    channel: Option<Channel>,
    attempt: u32,
}

impl ChatSession {
    async fn run(&mut self) {
        loop {
            if self.tx.is_closed() {
                return;
            }

            self.announce_status();

            let started_at = Instant::now();
            match self.connect_and_stream().await {
                Ok(StreamOutcome::Ended) => {
                    let _ = self
                        .tx
                        .send(AppEvent::Error("Live chat ended".into()))
                        .await;
                    let _ = self
                        .tx
                        .send(AppEvent::Status(StatusEvent::Disconnected))
                        .await;
                    return;
                }
                // The server closed the stream normally. This is routine, so
                // resubscribe without an error banner.
                Ok(StreamOutcome::Resubscribe) => {
                    self.attempt = 0;
                    // A stream that ends the instant it opens would otherwise
                    // spin here, hammering the endpoint and burning API quota.
                    if let Some(remaining) =
                        MIN_SUBSCRIPTION_INTERVAL.checked_sub(started_at.elapsed())
                    {
                        tokio::time::sleep(remaining).await;
                    }
                    continue;
                }
                Err(err) => {
                    if !self.handle_error(err).await {
                        return;
                    }
                }
            }
        }
    }

    fn announce_status(&self) {
        let status = if self.attempt == 0 {
            StatusEvent::Connecting
        } else {
            StatusEvent::Reconnecting {
                attempt: self.attempt,
            }
        };
        let _ = self.tx.try_send(AppEvent::Status(status));
    }

    async fn connect_and_stream(&mut self) -> Result<StreamOutcome, YoutubeError> {
        let channel = match self.channel.clone() {
            Some(channel) => channel,
            None => {
                let channel = self.yt.connect_chat_channel().await?;
                self.channel = Some(channel.clone());
                channel
            }
        };

        let outcome = self
            .yt
            .stream_chat(
                channel,
                &self.live_chat_id,
                &mut self.page_token,
                &mut self.dedup,
                &self.tx,
            )
            .await?;

        // A completed subscription proves the connection is healthy.
        self.attempt = 0;
        Ok(outcome)
    }

    /// Returns false when the loop should stop for good.
    async fn handle_error(&mut self, err: YoutubeError) -> bool {
        let recovery = err.recovery();
        warn!("chat stream failed ({recovery:?}): {err}");

        if recovery == Recovery::Fatal {
            let _ = self.tx.send(AppEvent::Error(err.to_string())).await;
            let _ = self
                .tx
                .send(AppEvent::Status(StatusEvent::Disconnected))
                .await;
            return false;
        }

        let _ = self
            .tx
            .send(AppEvent::Error(format!("chat disconnected: {err}")))
            .await;

        self.attempt = self.attempt.saturating_add(1);

        match recovery {
            Recovery::RefreshAuth => {
                if let Err(e) = self.yt.refresh_token().await {
                    warn!("token refresh failed: {e}");
                }
            }
            Recovery::ReResolve => self.re_resolve_chat_id().await,
            _ => {}
        }

        // Transport failures may have poisoned the channel; rebuild it.
        if matches!(recovery, Recovery::Retry | Recovery::ReResolve) {
            self.channel = None;
        }

        if self.attempt >= RE_RESOLVE_AFTER_ATTEMPTS
            && self.attempt.is_multiple_of(RE_RESOLVE_AFTER_ATTEMPTS)
            && recovery != Recovery::ReResolve
        {
            self.re_resolve_chat_id().await;
        }

        if self.tx.is_closed() {
            return false;
        }

        // Retry forever: giving up after five tries meant a single 30-second
        // outage killed chat for the rest of a multi-hour broadcast.
        let delay = reconnect_delay(self.attempt, u64::from(self.attempt) * 97);
        debug!("reconnecting in {:?} (attempt {})", delay, self.attempt);
        tokio::time::sleep(delay).await;

        true
    }

    /// A restarted broadcast gets a brand new `activeLiveChatId`; without this
    /// every reconnect would fail forever against the stale id.
    async fn re_resolve_chat_id(&mut self) {
        match self
            .yt
            .find_live_video_details_by_video_id(&self.video_id)
            .await
        {
            Ok(details) if details.chat_id != self.live_chat_id => {
                debug!("live chat id changed, resuming against the new chat");
                self.live_chat_id = details.chat_id;
                // A new chat means the old resume point is meaningless.
                self.page_token = None;
                self.channel = None;
                self.attempt = 0;
            }
            Ok(_) => debug!("live chat id unchanged"),
            Err(e) => debug!("could not re-resolve live chat id: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_then_caps() {
        let d = |attempt| reconnect_delay(attempt, 0);
        assert_eq!(d(0), Duration::from_secs(1));
        assert_eq!(d(1), Duration::from_secs(2));
        assert_eq!(d(2), Duration::from_secs(4));
        assert_eq!(d(3), Duration::from_secs(8));
        assert_eq!(d(4), Duration::from_secs(16));
        assert_eq!(d(5), Duration::from_secs(30));
        // Never grows past the cap, however long the outage lasts.
        assert_eq!(d(50), Duration::from_secs(30));
        assert_eq!(d(u32::MAX), Duration::from_secs(30));
    }

    #[test]
    fn jitter_stays_within_bounds() {
        for attempt in 0..10 {
            for seed in [0u64, 1, 124, 249, 999_999] {
                let delay = reconnect_delay(attempt, seed);
                assert!(
                    delay >= BASE_RECONNECT_DELAY,
                    "attempt {attempt} seed {seed}"
                );
                assert!(delay <= MAX_RECONNECT_DELAY, "attempt {attempt} seed {seed}");
            }
        }
    }

    #[test]
    fn jitter_actually_varies() {
        let a = reconnect_delay(3, 0);
        let b = reconnect_delay(3, 200);
        assert_ne!(a, b);
    }
}
