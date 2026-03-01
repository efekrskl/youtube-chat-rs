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
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui_image::picker::{Picker, ProtocolType};
use std::io::stdout;
use tokio::sync::mpsc;
use crate::stats_task::spawn_stats_task;

pub mod youtube_api_v3 {
    tonic::include_proto!("youtube.api.v3");
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
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

    enable_raw_mode()?;
    let _raw_mode_guard = RawModeGuard;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    if std::env::var("TERM_PROGRAM")
        .map(|value| value.contains("iTerm"))
        .unwrap_or(false)
    {
        picker.set_protocol_type(ProtocolType::Iterm2);
    }
    let (tx, rx) = mpsc::channel(100);

    spawn_input_task(tx.clone());
    spawn_stats_task(video_id, yt_service.clone(), tx.clone());
    spawn_youtube_chat_task(yt_service, live_video.chat_id, picker, tx);

    let app = App::new(live_video.channel_name);

    app.run(&mut terminal, rx).await?;
    terminal.show_cursor()?;
    Ok(())
}
