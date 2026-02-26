mod app;
mod input_task;
mod youtube;

use std::process::exit;
use crate::app::App;
use crate::input_task::spawn_input_task;
use crate::youtube::api::YoutubeService;
use crate::youtube::auth::auth;
use crate::youtube::spawn_youtube_chat_task;
use dialoguer::Input;
use log::debug;
use tokio::sync::mpsc;

pub mod youtube_api_v3 {
    tonic::include_proto!("youtube.api.v3");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
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
            let mut terminal = ratatui::init();
            let (tx, rx) = mpsc::channel(100);

            spawn_input_task(tx.clone());
            spawn_youtube_chat_task(yt_service, id, tx);

            let app = App::new(channel_name);

            app.run(&mut terminal, rx).await?;
            ratatui::restore();
            exit(0);
        }
        None => {
            debug!("no live chat id resolved");
            eprintln!("Couldn't find live chat id.");

            Ok(())
        }
    }
}
