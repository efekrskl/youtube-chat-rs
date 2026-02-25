use crate::http_types::{SearchResponse, VideoListResponse};
use anyhow::{Context, bail};
use reqwest::Url;
use reqwest::header::{AUTHORIZATION, HeaderValue};

pub struct AppClient {
    token: String,
    pub client: reqwest::Client,
}

impl AppClient {
    pub fn new(token: &str) -> anyhow::Result<AppClient> {
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
            client,
        })
    }
}

impl AppClient {
    fn auth_req(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.bearer_auth(&self.token)
    }

    async fn make_yt_req(&self, url: Url) -> anyhow::Result<String> {
        let res = self.auth_req(self.client.get(url)).send().await?;
        let status = res.status();
        let body = res.text().await?;

        if !status.is_success() {
            bail!("YouTube API error ({}): {}", status, body);
        }

        Ok(body)
    }

    async fn channel_id_by_name(&self, channel_name: &str) -> anyhow::Result<Option<String>> {
        let mut url = Url::parse("https://www.googleapis.com/youtube/v3/search")?;
        url.query_pairs_mut()
            .append_pair("part", "snippet")
            .append_pair("q", channel_name)
            .append_pair("type", "channel")
            .append_pair("maxResults", "1");

        let body = self.make_yt_req(url).await?;
        let parsed: SearchResponse = serde_json::from_str(&body)
            .context("Failed to parse search response (channel lookup)")?;

        Ok(parsed
            .items
            .get(0)
            .and_then(|i| i.id.as_ref())
            .and_then(|id| id.channel_id.clone()))
    }

    async fn live_video_id_by_channel_id(
        &self,
        channel_id: &str,
    ) -> anyhow::Result<Option<String>> {
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

        Ok(parsed
            .items
            .get(0)
            .and_then(|i| i.id.as_ref())
            .and_then(|id| id.video_id.clone()))
    }

    async fn find_chat_id_by_live_video_id(
        &self,
        live_video_id: &str,
    ) -> anyhow::Result<Option<String>> {
        let mut url = Url::parse("https://www.googleapis.com/youtube/v3/videos")?;
        url.query_pairs_mut()
            .append_pair("part", "liveStreamingDetails")
            .append_pair("id", &live_video_id);

        let body = self.make_yt_req(url).await?;
        let parsed: VideoListResponse = serde_json::from_str(&body)
            .context("Failed to parse search response (channel lookup)")?;

        Ok(parsed
            .items
            .get(0)
            .and_then(|v| v.live_streaming_details.as_ref())
            .and_then(|d| d.active_live_chat_id.clone()))
    }

    pub async fn find_chat_by_channel_name(
        &self,
        channel_name: &str,
    ) -> anyhow::Result<Option<String>> {
        let Some(channel_id) = self.channel_id_by_name(channel_name).await? else {
            return Ok(None);
        };
        println!("{channel_id}");

        let Some(live_stream_id) = self.live_video_id_by_channel_id(&channel_id).await? else {
            return Ok(None);
        };

        println!("{live_stream_id}");

        self.find_chat_id_by_live_video_id(&live_stream_id).await
    }
}
