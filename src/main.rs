mod auth;
mod client;
mod http_types;

use crate::auth::auth;
use crate::client::AppClient;
use crate::youtube_api_v3::LiveChatMessageListRequest;
use crate::youtube_api_v3::v3_data_live_chat_message_service_client::V3DataLiveChatMessageServiceClient;
use anyhow::Result;
use dialoguer::Input;
use tonic::Request;
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::transport::ClientTlsConfig;

pub mod youtube_api_v3 {
    tonic::include_proto!("youtube.api.v3");
}

#[tokio::main]
async fn main() -> Result<()> {
    let token = auth().await?;
    let app_client = AppClient::new(&token)?;
    let channel_name: String = Input::new()
        .allow_empty(false)
        .with_prompt("Please enter a channel name")
        .interact_text()?;
    let live_chat_id = app_client.find_chat_by_channel_name(&channel_name).await?;

    let tls = ClientTlsConfig::new().with_native_roots();
    let channel: Channel = Channel::from_static("https://youtube.googleapis.com")
        .tls_config(tls)?
        .connect()
        .await?;

    let mut client = V3DataLiveChatMessageServiceClient::new(channel);

    let mut next_page_token: Option<String> = None;

    loop {
        let req = LiveChatMessageListRequest {
            part: vec!["snippet".to_string()],
            live_chat_id: live_chat_id.clone(),
            max_results: Some(20),
            page_token: next_page_token.clone(),
            profile_image_size: Some(0),
            hl: Some("en".to_string()),
        };

        let mut request = Request::new(req);
        let auth: MetadataValue<_> = format!("Bearer {}", token).parse()?;
        request.metadata_mut().insert("authorization", auth);

        let mut stream = client.stream_list(request).await?.into_inner();

        while let Some(resp) = stream.message().await? {
            println!("{resp:?}");

            next_page_token = resp.next_page_token.clone();
        }

        if next_page_token.is_none() {
            break;
        }
    }

    Ok(())
}
