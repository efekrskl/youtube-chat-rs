use std::path::PathBuf;

use anyhow::Context;
use dialoguer::Input;
use log::debug;
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod};

const APP_DIR: &str = ".youtube-chat-rs";
const CLIENT_SECRET_FILE: &str = "client_secret.json";
const TOKEN_CACHE_FILE: &str = "token_cache.json";

fn resolve_paths() -> anyhow::Result<(PathBuf, PathBuf)> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    let base = PathBuf::from(home).join(APP_DIR);
    std::fs::create_dir_all(&base)
        .with_context(|| format!("Failed to create directory {}", base.display()))?;

    let client_secret_path = base.join(CLIENT_SECRET_FILE);
    let token_cache_path = base.join(TOKEN_CACHE_FILE);

    Ok((client_secret_path, token_cache_path))
}

pub async fn auth() -> anyhow::Result<String> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .unwrap();

    let (client_secret_path, token_cache_path) = resolve_paths()?;
    debug!("using client secret path: {}", client_secret_path.display());
    debug!("using token cache path: {}", token_cache_path.display());

    let client_secret_path = if client_secret_path.exists() {
        client_secret_path
    } else {
        let path: String = Input::new()
            .allow_empty(false)
            .with_prompt("Please enter the path of your client secret json file.")
            .interact_text()?;
        std::fs::copy(path, &client_secret_path)?;

        client_secret_path
    };

    let secret = yup_oauth2::read_application_secret(&client_secret_path).await?;

    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk(token_cache_path)
        .build()
        .await?;

    let scopes = &["https://www.googleapis.com/auth/youtube.readonly"];
    debug!("requesting OAuth token for readonly scope");

    let access_token = auth.token(scopes).await?;

    let token = match access_token.token() {
        Some(t) => t,
        None => panic!("Couldn't get oauth token"),
    };

    debug!("OAuth token acquired");
    Ok(token.to_string())
}
