mod auth;
mod client;
mod http_types;

use crate::auth::auth;
use crate::client::AppClient;
use dialoguer::Input;

pub mod youtube_api_v3 {
    tonic::include_proto!("youtube.api.v3");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let token = auth().await?;
    let app_client = AppClient::new(&token)?;
    let channel_name: String = Input::new()
        .allow_empty(false)
        .with_prompt("Please enter a channel name")
        .interact_text()?;
    let live_chat_id = app_client.find_chat_by_channel_name(&channel_name).await?;

    match live_chat_id {
        Some(id) => app_client.listen(&id).await,
        None => {
            eprintln!("Couldn't find live chat id.");

            Ok(())
        }
    }
}
