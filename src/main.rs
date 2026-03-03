mod app;
mod input_task;
mod youtube;
mod stats_task;

use clap::ArgGroup;
use clap::Parser;
use crate::app::App;
use crate::input_task::spawn_input_task;
use crate::youtube::api::YoutubeService;
use crate::youtube::auth::auth;
use crate::youtube::spawn_youtube_chat_task;
use log::debug;
use ratatui::crossterm::terminal::window_size;
use tokio::sync::mpsc;
use crate::stats_task::spawn_stats_task;

pub mod youtube_api_v3 {
    tonic::include_proto!("youtube.api.v3");
}
#[derive(Parser, Debug)]
#[command(
    name = "ytc",
    group(
        ArgGroup::new("input")
            .required(true)
            .args(["video", "channel"])
    )
)]
struct Args {
    /// Video ID
    #[arg(short = 'v', long = "video-id")]
    video: Option<String>,

    /// Channel Name
    #[arg(short = 'c', long = "channel-name")]
    channel: Option<String>,
}

const AVATAR_WIDTH: u16 = 2;
const AVATAR_HEIGHT: u16 = 1;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    debug!("application start");

    let token = auth().await?;
    let yt_service = YoutubeService::new(&token)?;
    let args = Args::parse();
    let video_id = match (args.video, args.channel) {
        (Some(video_id), None) => video_id,
        (None, Some(channel_name)) => {
            yt_service.find_video_id_by_channel_name(&channel_name).await?
        }
        _ => {
            unreachable!("Please pass either --video-id or --chanel-name")
        }
    };
    let live_video = yt_service.find_live_video_details_by_video_id(&video_id).await?;
    let window = window_size()?;
    let cell_width = (window.width / window.columns.max(1)).max(1);
    let cell_height = (window.height / window.rows.max(1)).max(1);
    let avatar_pixels = (cell_width * AVATAR_WIDTH, cell_height * AVATAR_HEIGHT);

    let mut terminal = ratatui::init();
    let (tx, rx) = mpsc::channel(100);

    spawn_input_task(tx.clone());
    spawn_stats_task(video_id, yt_service.clone(), tx.clone());
    spawn_youtube_chat_task(yt_service, live_video.chat_id, tx, avatar_pixels);

    let app = App::new(live_video.channel_name, avatar_pixels);

    app.run(&mut terminal, rx).await?;
    ratatui::restore();
    Ok(())
}
