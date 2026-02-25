mod auth;
mod client;
mod http_types;

use crate::auth::auth;
use crate::client::AppClient;
use dialoguer::Input;
use log::debug;

pub mod youtube_api_v3 {
    tonic::include_proto!("youtube.api.v3");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    debug!("application start");

    let token = auth().await?;
    let app_client = AppClient::new(&token)?;
    let channel_name: String = Input::new()
        .allow_empty(false)
        .with_prompt("Please enter a channel name")
        .interact_text()?;
    debug!("user input channel query: {}", channel_name);
    let live_chat_id = app_client.find_chat_by_channel_name(&channel_name).await?;

    match live_chat_id {
        Some(id) => {
            debug!("resolved live_chat_id, entering listen loop");
            app_client.listen(&id).await
        }
        None => {
            debug!("no live chat id resolved");
            eprintln!("Couldn't find live chat id.");

            Ok(())
        }
    }
}
