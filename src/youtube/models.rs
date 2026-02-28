use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct VideoListResponse {
    pub items: Vec<VideoItem>,
}

#[derive(Debug, Deserialize)]
pub struct VideoItem {
    #[serde(rename = "liveStreamingDetails")]
    pub live_streaming_details: Option<LiveStreamingDetails>,
    pub snippet: Option<VideoSnippet>,
}

#[derive(Debug, Deserialize)]
pub struct LiveStreamingDetails {
    #[serde(rename = "activeLiveChatId")]
    pub active_live_chat_id: Option<String>,

    #[serde(rename = "concurrentViewers")]
    pub concurrent_viewers: Option<String>
}

#[derive(Debug, Deserialize)]
pub struct VideoSnippet {
    #[serde(rename = "channelTitle")]
    pub channel_title: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
pub struct SearchResponse {
    pub items: Vec<SearchItem>,
}

#[derive(serde::Deserialize, Debug)]
pub struct SearchItem {
    pub id: Option<SearchId>,
}

#[derive(serde::Deserialize, Debug)]
pub struct SearchId {
    #[serde(rename = "videoId")]
    pub video_id: Option<String>,

    #[serde(rename = "channelId")]
    pub channel_id: Option<String>,
}
