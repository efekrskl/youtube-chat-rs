mod youtube;
mod ui;

use dialoguer::Input;
use log::debug;
use crate::youtube::api::YoutubeService;
use crate::youtube::auth::auth;

pub mod youtube_api_v3 {
    tonic::include_proto!("youtube.api.v3");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ui::run_ui();
    return Ok(());

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    debug!("application start");

    let token = auth().await?;
    let yt_service = YoutubeService::new(&token)?;
    let channel_name: String = Input::new()
        .allow_empty(false)
        .with_prompt("Please enter a channel name")
        .interact_text()?;
    debug!("user input channel query: {}", channel_name);
    let live_chat_id = yt_service.find_chat_by_channel_name(&channel_name).await?;

    match live_chat_id {
        Some(id) => {
            debug!("resolved live_chat_id, entering listen loop");
            yt_service.stream_chat(&id).await
        }
        None => {
            debug!("no live chat id resolved");
            eprintln!("Couldn't find live chat id.");

            Ok(())
        }
    }
}
