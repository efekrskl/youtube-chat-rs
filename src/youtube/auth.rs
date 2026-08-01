use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, bail};
use dialoguer::Input;
use log::debug;
use yup_oauth2::authenticator::DefaultAuthenticator;
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod};

const APP_DIR: &str = ".youtube-chat-rs";
const CLIENT_SECRET_FILE: &str = "client_secret.json";
const TOKEN_CACHE_FILE: &str = "token_cache.json";

pub const SCOPES: &[&str] = &["https://www.googleapis.com/auth/youtube.readonly"];

/// Directory holding the client secret, the token cache and the log file.
pub fn app_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    let base = PathBuf::from(home).join(APP_DIR);
    std::fs::create_dir_all(&base)
        .with_context(|| format!("Failed to create directory {}", base.display()))?;
    restrict_permissions(&base, 0o700);

    Ok(base)
}

/// Tighten permissions on files that hold OAuth credentials. Best effort: a
/// failure here must not stop the app, but it is worth logging.
fn restrict_permissions(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)) {
            debug!("could not restrict permissions on {}: {e}", path.display());
        }
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

fn resolve_paths() -> anyhow::Result<(PathBuf, PathBuf)> {
    let base = app_dir()?;

    Ok((base.join(CLIENT_SECRET_FILE), base.join(TOKEN_CACHE_FILE)))
}

/// Build an authenticator. The returned value is kept alive for the whole
/// process so that `token()` can transparently refresh the access token --
/// Google's access tokens expire after roughly an hour, which is well within a
/// single broadcast.
pub async fn auth() -> anyhow::Result<Arc<DefaultAuthenticator>> {
    // Installing the default crypto provider fails if one is already present,
    // which is not an error for us.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (client_secret_path, token_cache_path) = resolve_paths()?;
    debug!("using client secret path: {}", client_secret_path.display());
    debug!("using token cache path: {}", token_cache_path.display());

    if !client_secret_path.exists() {
        let path: String = Input::new()
            .allow_empty(false)
            .with_prompt("Please enter the path of your client secret json file.")
            .interact_text()?;
        std::fs::copy(&path, &client_secret_path)
            .with_context(|| format!("Failed to copy client secret from {path}"))?;
    }
    restrict_permissions(&client_secret_path, 0o600);

    let secret = yup_oauth2::read_application_secret(&client_secret_path)
        .await
        .with_context(|| {
            format!(
                "Failed to read client secret {}",
                client_secret_path.display()
            )
        })?;

    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk(&token_cache_path)
        .build()
        .await?;

    debug!("requesting OAuth token for readonly scope");

    // Force the interactive flow now (if needed) rather than in the middle of
    // the TUI, and verify the credentials actually work before we take over the
    // terminal.
    let access_token = auth.token(SCOPES).await?;
    if access_token.token().is_none() {
        bail!("OAuth provider returned an empty access token");
    }
    restrict_permissions(&token_cache_path, 0o600);

    debug!("OAuth token acquired");
    Ok(Arc::new(auth))
}
