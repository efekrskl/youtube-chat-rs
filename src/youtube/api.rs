use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::Context;
use log::debug;
use reqwest::Url;
use tokio::sync::mpsc;
use tonic::Request;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, ClientTlsConfig};
use yup_oauth2::authenticator::DefaultAuthenticator;

use crate::app::event::{AppEvent, StatusEvent};
use crate::youtube::auth::SCOPES;
use crate::youtube::avatar::AvatarService;
use crate::youtube::error::{YoutubeError, redact};
use crate::youtube::message::{MessageDedup, is_chat_ended, map_message};
use crate::youtube::models::{SearchResponse, VideoListResponse};
use crate::youtube_api_v3::LiveChatMessageListRequest;
use crate::youtube_api_v3::v3_data_live_chat_message_service_client::V3DataLiveChatMessageServiceClient;

/// A TLS handshake on a cold DNS cache regularly needs more than a second; the
/// old 1s budget made every reconnect fail exactly when the network was already
/// struggling.
const GRPC_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Google's frontends reject clients that ping more often than their minimum
/// interval with `GOAWAY / ENHANCE_YOUR_CALM (too_many_pings)`. Pinging once a
/// second was tearing the stream down on a regular cadence.
const GRPC_KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(30);
const GRPC_KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(20);
const TCP_KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(60);

const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

const YOUTUBE_GRPC_ENDPOINT: &str = "https://youtube.googleapis.com";
const MAX_MESSAGES_PER_PAGE: u32 = 200;

#[derive(Clone)]
pub struct YoutubeService {
    auth: Arc<DefaultAuthenticator>,
    http: reqwest::Client,
    pub avatars: AvatarService,
    /// Messages the UI was too slow to accept, reported as soon as it catches up.
    dropped: Arc<AtomicUsize>,
}

#[derive(Debug, Clone)]
pub struct LiveVideoDetails {
    pub chat_id: String,
    pub channel_name: String,
}

/// Outcome of one `StreamList` subscription.
pub enum StreamOutcome {
    /// The server closed the stream; resubscribe with the token we hold.
    Resubscribe,
    /// The chat is over for good.
    Ended,
}

impl YoutubeService {
    pub fn new(
        auth: Arc<DefaultAuthenticator>,
        avatar_dir: std::path::PathBuf,
    ) -> anyhow::Result<Self> {
        // No default Authorization header: this client is also used for URLs
        // that come out of API payloads, and the OAuth token must never leave
        // googleapis.com.
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .connect_timeout(HTTP_CONNECT_TIMEOUT)
            .user_agent(concat!("ytc/", env!("CARGO_PKG_VERSION")))
            .build()?;

        let avatar_http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .connect_timeout(HTTP_CONNECT_TIMEOUT)
            .user_agent(concat!("ytc/", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self {
            auth,
            http,
            avatars: AvatarService::new(avatar_http, avatar_dir),
            dropped: Arc::new(AtomicUsize::new(0)),
        })
    }

    /// Fetch a valid access token. `yup-oauth2` serves it from its cache and
    /// silently refreshes once it is close to expiry, which is what keeps a
    /// multi-hour broadcast alive.
    async fn access_token(&self) -> Result<String, YoutubeError> {
        let token = self
            .auth
            .token(SCOPES)
            .await
            .map_err(|e| YoutubeError::Auth(redact(&e.to_string())))?;

        token
            .token()
            .map(str::to_string)
            .ok_or_else(|| YoutubeError::Auth("empty access token".to_string()))
    }

    /// Discard the cached token and get a new one. Used after the server told
    /// us the current one is no longer accepted.
    pub async fn refresh_token(&self) -> Result<(), YoutubeError> {
        self.auth
            .force_refreshed_token(SCOPES)
            .await
            .map_err(|e| YoutubeError::Auth(redact(&e.to_string())))?;
        debug!("access token force-refreshed");
        Ok(())
    }

    async fn make_yt_req(&self, url: Url) -> Result<String, YoutubeError> {
        debug!("YouTube request: {}", url.path());
        let token = self.access_token().await?;

        let res = self.http.get(url).bearer_auth(token).send().await?;
        let status = res.status();
        let body = res.text().await?;
        debug!("YouTube response status={} body_len={}", status, body.len());

        if !status.is_success() {
            // The raw body can be kilobytes of JSON and may echo request
            // details; it ends up in a one-line status bar.
            return Err(YoutubeError::Api {
                status: status.as_u16(),
                message: redact(&body),
            });
        }

        Ok(body)
    }

    async fn channel_id_by_name(&self, channel_name: &str) -> Result<Option<String>, YoutubeError> {
        debug!("resolving channel by name");
        let mut url = Url::parse("https://www.googleapis.com/youtube/v3/search")
            .map_err(|e| YoutubeError::Other(e.into()))?;
        url.query_pairs_mut()
            .append_pair("part", "snippet")
            .append_pair("q", channel_name)
            .append_pair("type", "channel")
            .append_pair("maxResults", "1");

        let body = self.make_yt_req(url).await?;
        let parsed: SearchResponse = serde_json::from_str(&body)
            .context("Failed to parse search response (channel lookup)")
            .map_err(YoutubeError::Other)?;

        let channel_id = parsed
            .items
            .first()
            .and_then(|i| i.id.as_ref())
            .and_then(|id| id.channel_id.clone());
        debug!("channel lookup resolved={}", channel_id.is_some());

        Ok(channel_id)
    }

    async fn live_video_id_by_channel_id(
        &self,
        channel_id: &str,
    ) -> Result<Option<String>, YoutubeError> {
        debug!("resolving live video by channel id");
        let mut url = Url::parse("https://www.googleapis.com/youtube/v3/search")
            .map_err(|e| YoutubeError::Other(e.into()))?;
        url.query_pairs_mut()
            .append_pair("part", "id")
            .append_pair("channelId", channel_id)
            .append_pair("eventType", "live")
            .append_pair("type", "video")
            .append_pair("maxResults", "1");

        let body = self.make_yt_req(url).await?;
        let parsed: SearchResponse = serde_json::from_str(&body)
            .context("Failed to parse search response (live video lookup)")
            .map_err(YoutubeError::Other)?;

        let video_id = parsed
            .items
            .first()
            .and_then(|i| i.id.as_ref())
            .and_then(|id| id.video_id.clone());
        debug!("live video lookup resolved={}", video_id.is_some());

        Ok(video_id)
    }

    async fn chat_details_by_video_id(
        &self,
        live_video_id: &str,
    ) -> Result<Option<LiveVideoDetails>, YoutubeError> {
        debug!("resolving live chat by video id");
        let mut url = Url::parse("https://www.googleapis.com/youtube/v3/videos")
            .map_err(|e| YoutubeError::Other(e.into()))?;
        url.query_pairs_mut()
            .append_pair("part", "liveStreamingDetails,snippet")
            .append_pair("id", live_video_id);

        let body = self.make_yt_req(url).await?;
        let parsed: VideoListResponse = serde_json::from_str(&body)
            .context("Failed to parse video response (live chat lookup)")
            .map_err(YoutubeError::Other)?;

        let item = parsed.items.first();
        let chat_id = item
            .and_then(|v| v.live_streaming_details.as_ref())
            .and_then(|d| d.active_live_chat_id.clone());
        let channel_name = item
            .and_then(|v| v.snippet.as_ref())
            .and_then(|snippet| snippet.channel_title.clone());
        debug!("live chat lookup chat_id_found={}", chat_id.is_some());

        Ok(match (chat_id, channel_name) {
            (Some(chat_id), Some(channel_name)) => Some(LiveVideoDetails {
                chat_id,
                channel_name,
            }),
            _ => None,
        })
    }

    pub async fn find_video_id_by_channel_name(
        &self,
        channel_name: &str,
    ) -> Result<String, YoutubeError> {
        let Some(channel_id) = self.channel_id_by_name(channel_name).await? else {
            return Err(YoutubeError::NotLive);
        };

        let Some(live_stream_id) = self.live_video_id_by_channel_id(&channel_id).await? else {
            return Err(YoutubeError::NotLive);
        };

        Ok(live_stream_id)
    }

    pub async fn find_live_video_details_by_video_id(
        &self,
        live_stream_id: &str,
    ) -> Result<LiveVideoDetails, YoutubeError> {
        self.chat_details_by_video_id(live_stream_id)
            .await?
            .ok_or(YoutubeError::NotLive)
    }

    pub async fn get_viewer_count_by_video_id(
        &self,
        live_video_id: &str,
    ) -> Result<Option<u32>, YoutubeError> {
        let mut url = Url::parse("https://www.googleapis.com/youtube/v3/videos")
            .map_err(|e| YoutubeError::Other(e.into()))?;
        url.query_pairs_mut()
            .append_pair("part", "liveStreamingDetails")
            .append_pair("id", live_video_id);

        let body = self.make_yt_req(url).await?;
        let parsed: VideoListResponse = serde_json::from_str(&body)
            .context("Failed to parse video response (viewer count)")
            .map_err(YoutubeError::Other)?;

        Ok(parsed
            .items
            .first()
            .and_then(|v| v.live_streaming_details.as_ref())
            .and_then(|d| d.concurrent_viewers.as_deref())
            .and_then(|count| count.parse::<u32>().ok()))
    }

    /// Build the gRPC channel. Created once and reused across reconnects --
    /// tonic reconnects the underlying transport lazily, so we do not pay for
    /// DNS + TLS on every retry.
    pub async fn connect_chat_channel(&self) -> Result<Channel, YoutubeError> {
        let tls = ClientTlsConfig::new().with_native_roots();

        let channel = Channel::from_static(YOUTUBE_GRPC_ENDPOINT)
            .tls_config(tls)?
            .connect_timeout(GRPC_CONNECT_TIMEOUT)
            .tcp_keepalive(Some(TCP_KEEP_ALIVE_INTERVAL))
            .tcp_nodelay(true)
            .http2_keep_alive_interval(GRPC_KEEP_ALIVE_INTERVAL)
            .keep_alive_timeout(GRPC_KEEP_ALIVE_TIMEOUT)
            .keep_alive_while_idle(true)
            .http2_adaptive_window(true)
            .connect()
            .await?;

        debug!("gRPC channel connected");
        Ok(channel)
    }

    /// Consume one `StreamList` subscription, forwarding messages to the UI.
    ///
    /// `page_token` is owned by the caller so that a reconnect resumes exactly
    /// where the previous stream stopped instead of replaying (or skipping) the
    /// backlog.
    pub async fn stream_chat(
        &self,
        channel: Channel,
        live_chat_id: &str,
        page_token: &mut Option<String>,
        dedup: &mut MessageDedup,
        tx: &mpsc::Sender<AppEvent>,
    ) -> Result<StreamOutcome, YoutubeError> {
        let mut client = V3DataLiveChatMessageServiceClient::new(channel);

        // Hoisted out of the loop: these are identical on every cycle.
        let parts = vec![
            "id".to_string(),
            "snippet".to_string(),
            "authorDetails".to_string(),
        ];
        let token = self.access_token().await?;
        let auth: MetadataValue<_> = format!("Bearer {token}")
            .parse()
            .map_err(|_| YoutubeError::Auth("malformed access token".to_string()))?;

        let req = LiveChatMessageListRequest {
            part: parts,
            live_chat_id: Some(live_chat_id.to_string()),
            max_results: Some(MAX_MESSAGES_PER_PAGE),
            page_token: page_token.clone(),
            profile_image_size: Some(0),
            hl: Some("en".to_string()),
        };

        let mut request = Request::new(req);
        request.metadata_mut().insert("authorization", auth);

        let mut stream = client.stream_list(request).await?.into_inner();
        let _ = tx.try_send(AppEvent::Status(StatusEvent::Connected));

        while let Some(resp) = stream.message().await? {
            debug!(
                "stream page items={} next_page_token_present={}",
                resp.items.len(),
                resp.next_page_token.is_some()
            );

            // The broadcast itself ended; no amount of reconnecting helps.
            if resp.offline_at.is_some() {
                return Err(YoutubeError::StreamOffline);
            }

            let mut chat_ended = false;

            for item in resp.items.iter() {
                if is_chat_ended(item) {
                    chat_ended = true;
                }

                if !dedup.insert(item.id.as_deref()) {
                    continue;
                }

                let Some(mut msg) = map_message(item) else {
                    continue;
                };

                // Attach an already-cached avatar; otherwise kick off a
                // background fetch. Downloading inline used to stall the gRPC
                // reader, which the server then saw as a dead connection.
                if let Some(url) = msg.avatar_url.clone() {
                    match self.avatars.cached(&url) {
                        Some(avatar) => msg.avatar = Some(avatar),
                        None => self.spawn_avatar_fetch(url, tx.clone()),
                    }
                }

                self.emit(tx, AppEvent::Chat(msg))?;
            }

            // Only advance the resume point when the server gave us one.
            if resp.next_page_token.is_some() {
                *page_token = resp.next_page_token.clone();
            }

            if chat_ended {
                return Ok(StreamOutcome::Ended);
            }
        }

        // A completed server stream is normal: resubscribe from where we are.
        Ok(StreamOutcome::Resubscribe)
    }

    /// Send without ever blocking the gRPC reader. If the UI has fallen behind
    /// we count the drop rather than applying backpressure all the way into the
    /// HTTP/2 stream.
    fn emit(&self, tx: &mpsc::Sender<AppEvent>, event: AppEvent) -> Result<(), YoutubeError> {
        // A full channel means a `Dropped` event cannot get through either, so
        // the count is buffered here and reported once there is room again.
        let pending = self.dropped.swap(0, Ordering::Relaxed);
        if pending > 0 && tx.try_send(AppEvent::Dropped(pending)).is_err() {
            self.dropped.fetch_add(pending, Ordering::Relaxed);
        }

        match tx.try_send(event) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(YoutubeError::Other(anyhow::anyhow!("UI closed")))
            }
        }
    }

    fn spawn_avatar_fetch(&self, url: String, tx: mpsc::Sender<AppEvent>) {
        if !self.avatars.claim(&url) {
            return;
        }

        let avatars = self.avatars.clone();
        tokio::spawn(async move {
            if let Some(avatar) = avatars.fetch(&url).await {
                let _ = tx.send(AppEvent::AvatarReady { url, avatar }).await;
            }
        });
    }
}

/// YouTube video ids are 11 characters of URL-safe base64. Validating up front
/// turns a confusing "couldn't find live chat id" into a clear message.
pub fn validate_video_id(video_id: &str) -> Result<(), String> {
    if video_id.len() == 11
        && video_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Ok(());
    }

    Err(format!(
        "'{}' is not a valid YouTube video id (expected 11 characters of A-Z, a-z, 0-9, '-' or '_')",
        redact(video_id)
    ))
}

/// Channel names go into a query string, so they are escaped rather than
/// injected -- this only guards against absurd input.
pub fn validate_channel_name(channel_name: &str) -> Result<(), String> {
    let trimmed = channel_name.trim();
    if trimmed.is_empty() {
        return Err("channel name is empty".to_string());
    }
    if trimmed.chars().count() > 100 {
        return Err("channel name is too long (max 100 characters)".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_well_formed_video_ids() {
        assert!(validate_video_id("dQw4w9WgXcQ").is_ok());
        assert!(validate_video_id("a-b_c1234_5").is_ok());
    }

    #[test]
    fn rejects_malformed_video_ids() {
        assert!(validate_video_id("").is_err());
        assert!(validate_video_id("too-short").is_err());
        assert!(validate_video_id("waaaaaaaaaaaaaay-too-long").is_err());
        assert!(validate_video_id("bad/char/x1").is_err());
        // A full URL is a common mistake and must be rejected clearly.
        assert!(validate_video_id("https://youtu.be/dQw4w9WgXcQ").is_err());
    }

    #[test]
    fn validates_channel_names() {
        assert!(validate_channel_name("Some Channel").is_ok());
        assert!(validate_channel_name("   ").is_err());
        assert!(validate_channel_name(&"a".repeat(101)).is_err());
    }
}
