mod app;
mod input_task;
mod stats_task;
mod youtube;

use std::path::PathBuf;

use anyhow::Context;
use clap::ArgGroup;
use clap::Parser;
use log::debug;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::event::AppEvent;
use crate::input_task::spawn_input_task;
use crate::stats_task::spawn_stats_task;
use crate::youtube::api::{YoutubeService, validate_channel_name, validate_video_id};
use crate::youtube::auth::{app_dir, auth};
use crate::youtube::spawn_youtube_chat_task;

pub mod youtube_api_v3 {
    tonic::include_proto!("youtube.api.v3");
}

/// Room for a burst of chat without the producer ever having to block.
const EVENT_CHANNEL_CAPACITY: usize = 1024;

#[derive(Parser, Debug)]
#[command(
    name = "ytc",
    version,
    about = "YouTube live chat in the terminal.",
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
    // Parse before authenticating: `--help` and `--version` should not trigger
    // an OAuth browser flow.
    let args = Args::parse();

    // Reject obviously bad input before the OAuth flow, so a typo fails
    // immediately instead of after a browser round trip.
    match (&args.video, &args.channel) {
        (Some(video_id), _) => validate_video_id(video_id),
        (None, Some(channel_name)) => validate_channel_name(channel_name),
        // clap's ArgGroup guarantees one of the two is present.
        (None, None) => unreachable!("clap requires --video-id or --channel-name"),
    }
    .map_err(anyhow::Error::msg)?;

    let log_path = init_logging()?;
    debug!("application start");

    let auth = auth().await?;
    let avatar_dir = avatar_dir()?;
    let yt_service = YoutubeService::new(auth, avatar_dir)?;

    let video_id = match (&args.video, &args.channel) {
        (Some(video_id), _) => video_id.clone(),
        (None, Some(channel_name)) => {
            yt_service
                .find_video_id_by_channel_name(channel_name)
                .await?
        }
        (None, None) => unreachable!("clap requires --video-id or --channel-name"),
    };

    let live_video = yt_service
        .find_live_video_details_by_video_id(&video_id)
        .await?;

    install_panic_hook();
    let mut terminal = ratatui::init();
    let (tx, rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);

    spawn_input_task(tx.clone());
    spawn_stats_task(video_id.clone(), yt_service.clone(), tx.clone());
    let chat_task = spawn_youtube_chat_task(yt_service, video_id, live_video.chat_id, tx.clone());

    // If the chat task ever dies unexpectedly (a panic used to end the chat
    // with no message at all), say so instead of leaving a frozen window.
    tokio::spawn({
        let tx = tx.clone();
        async move {
            if let Err(e) = chat_task.await {
                let _ = tx
                    .send(AppEvent::Error(format!(
                        "chat task stopped unexpectedly: {e}"
                    )))
                    .await;
            }
        }
    });
    drop(tx);

    let app = App::new(live_video.channel_name);
    let result = app.run(&mut terminal, rx).await;

    ratatui::restore();
    if let Err(e) = &result {
        eprintln!("ytc exited with an error: {e}");
        eprintln!("logs: {}", log_path.display());
    }

    result
}

/// The TUI owns the terminal, so logging to stderr would scribble over it.
fn init_logging() -> anyhow::Result<PathBuf> {
    let path = app_dir()?.join("ytc.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("Failed to open log file {}", path.display()))?;

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Pipe(Box::new(file)))
        .init();

    Ok(path)
}

/// Avatars are written to a private per-user directory rather than a
/// predictable path in a shared `/tmp`.
fn avatar_dir() -> anyhow::Result<PathBuf> {
    let dir = app_dir()?.join("avatars");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create directory {}", dir.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }

    Ok(dir)
}

/// Without this a panic leaves the terminal in raw mode on the alternate
/// screen, which looks like the shell itself broke.
fn install_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        hook(info);
    }));
}
