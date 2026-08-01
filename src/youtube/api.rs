use crate::app::event::{AppEvent, KittyAvatar, StatusEvent};
use crate::youtube::models::{SearchResponse, VideoListResponse};
use crate::youtube_api_v3::LiveChatMessageListRequest;
use crate::youtube_api_v3::v3_data_live_chat_message_service_client::V3DataLiveChatMessageServiceClient;
use crate::youtube::auth::SCOPES;
use crate::youtube::error::YoutubeError;
use crate::youtube::message::{MessageDedup, is_chat_ended, map_message};
use anyhow::{Context, bail};
use image::imageops::FilterType;
use log::debug;
use reqwest::Url;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tonic::Request;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, ClientTlsConfig};
use yup_oauth2::authenticator::DefaultAuthenticator;

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

/// Outcome of one `StreamList` subscription.
pub enum StreamOutcome {
    /// The server closed the stream; resubscribe with the token we hold.
    Resubscribe,
    /// The chat is over for good.
    Ended,
}

#[derive(Clone)]
pub struct YoutubeService {
    auth: Arc<DefaultAuthenticator>,
    pub http: reqwest::Client,
}

pub struct LiveVideoDetails {
    pub chat_id: String,
    pub channel_name: String,
}

impl YoutubeService {
    pub fn new(auth: Arc<DefaultAuthenticator>) -> anyhow::Result<YoutubeService> {
        // No default Authorization header: this client also fetches avatar URLs
        // that come out of API payloads, and the OAuth token must never leave
        // googleapis.com.
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .connect_timeout(HTTP_CONNECT_TIMEOUT)
            .user_agent(concat!("ytc/", env!("CARGO_PKG_VERSION")))
            .build()?;

        Ok(Self { auth, http })
    }
}

impl YoutubeService {
    /// Fetch a valid access token. `yup-oauth2` serves it from its cache and
    /// silently refreshes once it is close to expiry, which is what keeps a
    /// multi-hour broadcast alive.
    async fn access_token(&self) -> anyhow::Result<String> {
        let token = self.auth.token(SCOPES).await?;

        match token.token() {
            Some(token) => Ok(token.to_string()),
            None => bail!("OAuth provider returned an empty access token"),
        }
    }

    /// Discard the cached token and get a new one. Used after the server told
    /// us the current one is no longer accepted.
    pub async fn refresh_token(&self) -> Result<(), YoutubeError> {
        self.auth
            .force_refreshed_token(SCOPES)
            .await
            .map_err(|e| YoutubeError::Auth(e.to_string()))?;
        debug!("access token force-refreshed");
        Ok(())
    }

    async fn make_yt_req(&self, url: Url) -> anyhow::Result<String> {
        debug!("YouTube request: {}", url);
        let token = self.access_token().await?;
        let res = self.http.get(url).bearer_auth(token).send().await?;
        let status = res.status();
        let body = res.text().await?;
        debug!("YouTube response status={} body_len={}", status, body.len());

        if !status.is_success() {
            bail!("YouTube API error ({}): {}", status, body);
        }

        Ok(body)
    }

    async fn channel_id_by_name(&self, channel_name: &str) -> anyhow::Result<Option<String>> {
        debug!("resolving channel by name query={}", channel_name);
        let mut url = Url::parse("https://www.googleapis.com/youtube/v3/search")?;
        url.query_pairs_mut()
            .append_pair("part", "snippet")
            .append_pair("q", channel_name)
            .append_pair("type", "channel")
            .append_pair("maxResults", "1");

        let body = self.make_yt_req(url).await?;
        let parsed: SearchResponse = serde_json::from_str(&body)
            .context("Failed to parse search response (channel lookup)")?;

        let channel_id = parsed
            .items
            .get(0)
            .and_then(|i| i.id.as_ref())
            .and_then(|id| id.channel_id.clone());
        debug!("channel lookup result={:?}", channel_id);

        Ok(channel_id)
    }

    async fn live_video_id_by_channel_id(
        &self,
        channel_id: &str,
    ) -> anyhow::Result<Option<String>> {
        debug!("resolving live video by channel_id={}", channel_id);
        let mut url = Url::parse("https://www.googleapis.com/youtube/v3/search")?;
        url.query_pairs_mut()
            .append_pair("part", "id")
            .append_pair("channelId", channel_id)
            .append_pair("eventType", "live")
            .append_pair("type", "video")
            .append_pair("maxResults", "1");

        let body = self.make_yt_req(url).await?;
        let parsed: SearchResponse = serde_json::from_str(&body)
            .context("Failed to parse search response (live video lookup)")?;

        let video_id = parsed
            .items
            .get(0)
            .and_then(|i| i.id.as_ref())
            .and_then(|id| id.video_id.clone());
        debug!("live video lookup result={:?}", video_id);

        Ok(video_id)
    }

    async fn find_chat_id_by_live_video_id(
        &self,
        live_video_id: &str,
    ) -> anyhow::Result<Option<LiveVideoDetails>> {
        debug!("resolving live chat by video_id={}", live_video_id);
        let mut url = Url::parse("https://www.googleapis.com/youtube/v3/videos")?;
        url.query_pairs_mut()
            .append_pair("part", "liveStreamingDetails,snippet")
            .append_pair("id", &live_video_id);

        let body = self.make_yt_req(url).await?;
        let parsed: VideoListResponse = serde_json::from_str(&body)
            .context("Failed to parse search response (channel lookup)")?;

        let item = parsed.items.get(0);
        let chat_id = item
            .and_then(|v| v.live_streaming_details.as_ref())
            .and_then(|d| d.active_live_chat_id.clone());
        let channel_name = item
            .and_then(|v| v.snippet.as_ref())
            .and_then(|snippet| snippet.channel_title.clone());
        debug!(
            "live chat lookup result chat_id={:?} channel_name={:?}",
            chat_id, channel_name
        );

        match (chat_id, channel_name) {
            (Some(chat_id), Some(channel_name)) => Ok(Some(LiveVideoDetails {
                chat_id,
                channel_name,
            })),
            _ => Ok(None),
        }
    }

    pub async fn find_video_id_by_channel_name(
        &self,
        channel_name: &str,
    ) -> anyhow::Result<String> {
        let Some(channel_id) = self.channel_id_by_name(channel_name).await? else {
            bail!("Couldn't find channel name");
        };
        debug!("resolved channel_id={}", channel_id);

        let Some(live_stream_id) = self.live_video_id_by_channel_id(&channel_id).await? else {
            bail!("Couldn't find live stream id");
        };
        debug!("resolved live_stream_id={}", live_stream_id);

        Ok(live_stream_id)
    }

    pub async fn find_live_video_details_by_video_id(
        &self,
        live_stream_id: &str,
    ) -> anyhow::Result<LiveVideoDetails> {
        let Some(details) = self.find_chat_id_by_live_video_id(&live_stream_id).await? else {
            bail!("Couldn't find live chat id");
        };
        debug!(
            "resolved chat_id={} channel_name={}",
            details.chat_id, details.channel_name
        );

        Ok(details)
    }

    pub async fn get_viewer_count_by_video_id(
        &self,
        live_video_id: &str,
    ) -> anyhow::Result<String> {
        debug!("resolving viewer count by video_id={}", live_video_id);
        let mut url = Url::parse("https://www.googleapis.com/youtube/v3/videos")?;
        url.query_pairs_mut()
            .append_pair("part", "liveStreamingDetails")
            .append_pair("id", &live_video_id);

        let body = self.make_yt_req(url).await?;
        let parsed: VideoListResponse = serde_json::from_str(&body)
            .context("Failed to parse search response (channel lookup)")?;

        let viewer_count = parsed
            .items
            .get(0)
            .and_then(|v| v.live_streaming_details.as_ref())
            .and_then(|d| d.concurrent_viewers.clone());
        debug!("live chat lookup result={:?}", viewer_count);

        Ok(viewer_count.unwrap_or("0".to_string()))
    }
}

impl YoutubeService {
    async fn fetch_avatar(&self, avatar_url: &str) -> Option<KittyAvatar> {
        let response = self.http.get(avatar_url).send().await.ok()?;
        let bytes = response.bytes().await.ok()?;
        let image = image::load_from_memory(&bytes).ok()?;
        let resized = image.resize_to_fill(32, 32, FilterType::Lanczos3);
        let rgba = resized.to_rgba8();
        let (width, height) = rgba.dimensions();

        let mut hasher = DefaultHasher::new();
        avatar_url.hash(&mut hasher);
        let id = hasher.finish() as u32 & 0x00FF_FFFF;

        let path = std::env::temp_dir().join(format!("ytc-kitty-avatar-{id}.rgba"));
        let mut file = File::create(&path).ok()?;
        file.write_all(rgba.as_raw()).ok()?;

        Some(KittyAvatar {
            id,
            cols: 2,
            width,
            height,
            path: path.to_string_lossy().into_owned(),
        })
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
    /// `page_token` and `dedup` are owned by the caller so that a reconnect
    /// resumes exactly where the previous stream stopped instead of replaying
    /// (or skipping) the backlog.
    pub async fn stream_chat(
        &self,
        channel: Channel,
        live_chat_id: &str,
        page_token: &mut Option<String>,
        dedup: &mut MessageDedup,
        avatar_cache: &mut HashMap<String, KittyAvatar>,
        tx: &mpsc::Sender<AppEvent>,
    ) -> Result<StreamOutcome, YoutubeError> {
        let mut client = V3DataLiveChatMessageServiceClient::new(channel);

        // Hoisted out of the old per-cycle loop: these are identical every time.
        let parts = vec![
            "id".to_string(),
            "snippet".to_string(),
            "authorDetails".to_string(),
        ];
        let token = self.access_token().await.map_err(YoutubeError::Other)?;
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
        let _ = tx.send(AppEvent::Status(StatusEvent::Connected)).await;

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

                // Resuming from a page token replays part of the backlog, so
                // ids we have already rendered must be skipped.
                if !dedup.insert(item.id.as_deref()) {
                    continue;
                }

                let Some(mut msg) = map_message(item) else {
                    continue;
                };

                if let Some(url) = msg.avatar_url.clone() {
                    msg.avatar = if let Some(cached) = avatar_cache.get(&url) {
                        Some(Arc::new(cached.clone()))
                    } else if let Some(fetched) = self.fetch_avatar(&url).await {
                        avatar_cache.insert(url, fetched.clone());
                        Some(Arc::new(fetched))
                    } else {
                        None
                    };
                }

                tx.send(AppEvent::Chat(msg))
                    .await
                    .map_err(|_| YoutubeError::Other(anyhow::anyhow!("UI closed")))?;
            }

            // Only advance the resume point when the server gave us one; a
            // final page without a token used to wipe a perfectly good one.
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
}
