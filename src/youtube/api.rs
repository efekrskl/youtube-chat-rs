use crate::app::event::{AppEvent, ChatMessage, MessageKind};
use crate::youtube::models::{SearchResponse, VideoListResponse};
use crate::youtube_api_v3::LiveChatMessageListRequest;
use crate::youtube_api_v3::v3_data_live_chat_message_service_client::V3DataLiveChatMessageServiceClient;
use anyhow::{Context, bail};
use log::debug;
use reqwest::Url;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use tokio::sync::mpsc;
use tonic::Request;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, ClientTlsConfig};

pub struct YoutubeService {
    token: String,
    pub http: reqwest::Client,
}

impl YoutubeService {
    pub fn new(token: &str) -> anyhow::Result<YoutubeService> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self {
            token: token.to_string(),
            http: client,
        })
    }
}

impl YoutubeService {
    fn auth_req(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.bearer_auth(&self.token)
    }

    async fn make_yt_req(&self, url: Url) -> anyhow::Result<String> {
        debug!("YouTube request: {}", url);
        let res = self.auth_req(self.http.get(url)).send().await?;
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
    ) -> anyhow::Result<Option<String>> {
        debug!("resolving live chat by video_id={}", live_video_id);
        let mut url = Url::parse("https://www.googleapis.com/youtube/v3/videos")?;
        url.query_pairs_mut()
            .append_pair("part", "liveStreamingDetails")
            .append_pair("id", &live_video_id);

        let body = self.make_yt_req(url).await?;
        let parsed: VideoListResponse = serde_json::from_str(&body)
            .context("Failed to parse search response (channel lookup)")?;

        let chat_id = parsed
            .items
            .get(0)
            .and_then(|v| v.live_streaming_details.as_ref())
            .and_then(|d| d.active_live_chat_id.clone());
        debug!("live chat lookup result={:?}", chat_id);

        Ok(chat_id)
    }

    pub async fn find_chat_by_channel_name(
        &self,
        channel_name: &str,
    ) -> anyhow::Result<Option<String>> {
        let Some(channel_id) = self.channel_id_by_name(channel_name).await? else {
            debug!("no channel_id found");
            return Ok(None);
        };
        debug!("resolved channel_id={}", channel_id);

        let Some(live_stream_id) = self.live_video_id_by_channel_id(&channel_id).await? else {
            debug!("no live video found for channel_id={}", channel_id);
            return Ok(None);
        };
        debug!("resolved live_stream_id={}", live_stream_id);

        let chat_id = self.find_chat_id_by_live_video_id(&live_stream_id).await?;
        debug!("resolved chat_id={:?}", chat_id);
        Ok(chat_id)
    }
}

impl YoutubeService {
    pub async fn stream_chat(
        &self,
        live_chat_id: &str,
        tx: mpsc::Sender<AppEvent>,
    ) -> anyhow::Result<()> {
        debug!("listen start live_chat_id={}", live_chat_id);
        let tls = ClientTlsConfig::new().with_native_roots();
        let channel: Channel = Channel::from_static("https://youtube.googleapis.com")
            .tls_config(tls)?
            .connect()
            .await?;
        debug!("gRPC channel connected");

        let mut client = V3DataLiveChatMessageServiceClient::new(channel);

        let mut next_page_token: Option<String> = None;
        let mut poll_cycle: usize = 0;

        loop {
            poll_cycle += 1;
            debug!(
                "stream poll cycle={} page_token_present={}",
                poll_cycle,
                next_page_token.is_some()
            );
            let req = LiveChatMessageListRequest {
                part: vec![
                    "id".to_string(),
                    "snippet".to_string(),
                    "authorDetails".to_string(),
                ],
                live_chat_id: Some(live_chat_id.to_string()),
                max_results: Some(20),
                page_token: next_page_token.clone(),
                profile_image_size: Some(0),
                hl: Some("en".to_string()),
            };

            let mut request = Request::new(req);
            let auth: MetadataValue<_> = format!("Bearer {}", self.token).parse()?;
            request.metadata_mut().insert("authorization", auth);

            let mut stream = client.stream_list(request).await?.into_inner();
            let mut got_page = false;

            while let Some(resp) = stream.message().await? {
                got_page = true;
                debug!(
                    "stream page items={} next_page_token_present={}",
                    resp.items.len(),
                    resp.next_page_token.is_some()
                );
                for item in resp.items.iter() {
                    use crate::youtube_api_v3::live_chat_message_snippet::type_wrapper::Type as MessageType;

                    let Some(snippet) = item.snippet.as_ref() else {
                        debug!("skipping item without snippet");
                        continue;
                    };

                    match snippet.r#type() {
                        MessageType::TextMessageEvent => {
                            // todo: get these properly
                            let message = snippet
                                .display_message
                                .as_deref()
                                .unwrap_or("<empty>")
                                .to_string();
                            let author = item
                                .author_details
                                .as_ref()
                                .and_then(|d| d.display_name.as_ref())
                                .map(String::as_str)
                                .unwrap_or("<unknown>")
                                .to_string();
                            let timestamp = snippet
                                .published_at
                                .as_deref()
                                .unwrap()
                                .get(11..16)
                                .unwrap_or("--:--")
                                .to_string();

                            tx.send(AppEvent::Chat(ChatMessage {
                                author,
                                message,
                                kind: MessageKind::Text,
                                timestamp,
                            }))
                            .await?;
                        }
                        MessageType::NewSponsorEvent => {}
                        _ => {}
                    }
                }

                next_page_token = resp.next_page_token.clone();
            }

            if !got_page {
                debug!("stream produced no pages in this cycle");
            }

            if next_page_token.is_none() {
                debug!("next_page_token absent, exiting listen loop");
                break;
            }
        }

        debug!("listen finished");
        Ok(())
    }
}
