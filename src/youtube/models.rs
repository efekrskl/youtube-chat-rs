use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct VideoListResponse {
    #[serde(default)]
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
    pub concurrent_viewers: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VideoSnippet {
    #[serde(rename = "channelTitle")]
    pub channel_title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub items: Vec<SearchItem>,
}

#[derive(Debug, Deserialize)]
pub struct SearchItem {
    pub id: Option<SearchId>,
}

#[derive(Debug, Deserialize)]
pub struct SearchId {
    #[serde(rename = "videoId")]
    pub video_id: Option<String>,

    #[serde(rename = "channelId")]
    pub channel_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/");
        std::fs::read_to_string(format!("{path}{name}"))
            .unwrap_or_else(|e| panic!("missing fixture {name}: {e}"))
    }

    #[test]
    fn parses_a_live_video_response() {
        let parsed: VideoListResponse =
            serde_json::from_str(&fixture("videos_live.json")).expect("should parse");

        let item = parsed.items.first().expect("one item");
        assert_eq!(
            item.live_streaming_details
                .as_ref()
                .unwrap()
                .active_live_chat_id
                .as_deref(),
            Some("Cg0KC2RRdzR3OVdnWGNR")
        );
        assert_eq!(
            item.live_streaming_details
                .as_ref()
                .unwrap()
                .concurrent_viewers
                .as_deref(),
            Some("1337")
        );
        assert_eq!(
            item.snippet.as_ref().unwrap().channel_title.as_deref(),
            Some("Test Channel")
        );
    }

    // A video that is not currently live has no liveStreamingDetails at all;
    // this must parse rather than error.
    #[test]
    fn parses_a_video_without_live_details() {
        let parsed: VideoListResponse =
            serde_json::from_str(&fixture("videos_not_live.json")).expect("should parse");

        let item = parsed.items.first().expect("one item");
        assert!(item.live_streaming_details.is_none());
        assert_eq!(
            item.snippet.as_ref().unwrap().channel_title.as_deref(),
            Some("Test Channel")
        );
    }

    // A live stream can be running without reporting a viewer count.
    #[test]
    fn parses_live_details_without_a_viewer_count() {
        let parsed: VideoListResponse =
            serde_json::from_str(&fixture("videos_no_viewers.json")).expect("should parse");

        let details = parsed.items[0].live_streaming_details.as_ref().unwrap();
        assert!(details.concurrent_viewers.is_none());
        assert!(details.active_live_chat_id.is_some());
    }

    #[test]
    fn parses_an_empty_result_set() {
        let parsed: VideoListResponse =
            serde_json::from_str(&fixture("videos_empty.json")).expect("should parse");
        assert!(parsed.items.is_empty());

        let parsed: SearchResponse =
            serde_json::from_str(&fixture("search_empty.json")).expect("should parse");
        assert!(parsed.items.is_empty());
    }

    #[test]
    fn a_missing_items_key_is_treated_as_empty() {
        let parsed: VideoListResponse = serde_json::from_str("{}").expect("should parse");
        assert!(parsed.items.is_empty());

        let parsed: SearchResponse = serde_json::from_str("{}").expect("should parse");
        assert!(parsed.items.is_empty());
    }

    #[test]
    fn parses_a_channel_search_response() {
        let parsed: SearchResponse =
            serde_json::from_str(&fixture("search_channel.json")).expect("should parse");

        let id = parsed.items[0].id.as_ref().unwrap();
        assert_eq!(id.channel_id.as_deref(), Some("UC_test_channel"));
        assert!(id.video_id.is_none());
    }

    #[test]
    fn parses_a_video_search_response() {
        let parsed: SearchResponse =
            serde_json::from_str(&fixture("search_video.json")).expect("should parse");

        let id = parsed.items[0].id.as_ref().unwrap();
        assert_eq!(id.video_id.as_deref(), Some("dQw4w9WgXcQ"));
        assert!(id.channel_id.is_none());
    }

    // Unknown fields appear whenever Google extends the API; they must not
    // break parsing.
    #[test]
    fn tolerates_unknown_fields() {
        let json = r#"{"kind":"youtube#videoListResponse","brandNewField":42,
            "items":[{"snippet":{"channelTitle":"X","somethingNew":true}}]}"#;
        let parsed: VideoListResponse = serde_json::from_str(json).expect("should parse");
        assert_eq!(
            parsed.items[0]
                .snippet
                .as_ref()
                .unwrap()
                .channel_title
                .as_deref(),
            Some("X")
        );
    }
}
