use thiserror::Error;
use tonic::Code;

/// How the reconnect supervisor should react to a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// Transient: back off and reconnect.
    Retry,
    /// The credentials went stale; refresh the token and retry immediately.
    RefreshAuth,
    /// The chat id we hold is no longer valid; re-resolve it before retrying.
    ReResolve,
    /// Nothing we do will help; stop and tell the user.
    Fatal,
}

#[derive(Debug, Error)]
pub enum YoutubeError {
    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("YouTube API quota exceeded, try again later")]
    QuotaExceeded,

    #[error("this live chat is no longer available")]
    ChatUnavailable,

    #[error("the stream went offline")]
    StreamOffline,

    #[error("YouTube API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("connection problem: {0}")]
    Transport(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl YoutubeError {
    pub fn recovery(&self) -> Recovery {
        match self {
            // A refreshed token is likely to fix this; yup-oauth2 refreshes
            // automatically, so a single immediate retry is the right move.
            YoutubeError::Auth(_) => Recovery::RefreshAuth,
            // Quota resets on a timer, so retrying with backoff is the only
            // option -- but it must not be a hot loop.
            YoutubeError::QuotaExceeded => Recovery::Retry,
            // The broadcast may have restarted with a fresh live chat id.
            YoutubeError::ChatUnavailable => Recovery::ReResolve,
            YoutubeError::StreamOffline => Recovery::Fatal,
            YoutubeError::Api { status, .. } => match status {
                401 | 403 => Recovery::RefreshAuth,
                404 => Recovery::ReResolve,
                408 | 429 | 500..=599 => Recovery::Retry,
                _ => Recovery::Fatal,
            },
            YoutubeError::Transport(_) => Recovery::Retry,
            YoutubeError::Other(_) => Recovery::Retry,
        }
    }
}

impl From<tonic::Status> for YoutubeError {
    fn from(status: tonic::Status) -> Self {
        let message = redact(status.message());

        match status.code() {
            Code::Unauthenticated => YoutubeError::Auth(message),
            // The Data API reports both "forbidden" and "over quota" as
            // PERMISSION_DENIED / RESOURCE_EXHAUSTED; only the latter is worth
            // waiting out.
            Code::ResourceExhausted => YoutubeError::QuotaExceeded,
            Code::PermissionDenied => YoutubeError::Auth(message),
            Code::NotFound => YoutubeError::ChatUnavailable,
            Code::InvalidArgument | Code::FailedPrecondition => YoutubeError::ChatUnavailable,
            Code::Unavailable
            | Code::Internal
            | Code::DeadlineExceeded
            | Code::Aborted
            | Code::Cancelled
            | Code::Unknown => YoutubeError::Transport(message),
            _ => YoutubeError::Api {
                status: status.code() as u16,
                message,
            },
        }
    }
}

impl From<tonic::transport::Error> for YoutubeError {
    fn from(e: tonic::transport::Error) -> Self {
        YoutubeError::Transport(redact(&e.to_string()))
    }
}

impl From<reqwest::Error> for YoutubeError {
    fn from(e: reqwest::Error) -> Self {
        YoutubeError::Transport(redact(&e.to_string()))
    }
}

/// Error text is rendered into a single-line status bar and written to the log,
/// so keep it short and strip anything that looks like a credential.
pub fn redact(message: &str) -> String {
    const MAX_LEN: usize = 200;

    let mut out = String::with_capacity(message.len().min(MAX_LEN + 3));
    for (i, word) in message.split_whitespace().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if looks_like_secret(word) {
            out.push_str("<redacted>");
        } else {
            out.push_str(word);
        }
    }

    truncate(&out, MAX_LEN)
}

fn looks_like_secret(word: &str) -> bool {
    let trimmed = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    trimmed.starts_with("ya29.")
        || trimmed.starts_with("AIza")
        || trimmed.starts_with("1//")
        || word.to_ascii_lowercase().starts_with("bearer")
}

/// Truncate on a char boundary so we never split a multi-byte character.
pub fn truncate(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }

    let end = (0..=max_len)
        .rev()
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(0);

    format!("{}...", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unauthenticated_asks_for_a_token_refresh() {
        let err: YoutubeError = tonic::Status::unauthenticated("token expired").into();
        assert_eq!(err.recovery(), Recovery::RefreshAuth);
    }

    // Regression: a GOAWAY used to be indistinguishable from a fatal error and
    // burned one of only five reconnect attempts.
    #[test]
    fn transport_failures_are_retried() {
        let err: YoutubeError = tonic::Status::unavailable("GOAWAY").into();
        assert_eq!(err.recovery(), Recovery::Retry);
    }

    #[test]
    fn quota_exhaustion_is_retried_not_fatal() {
        let err: YoutubeError = tonic::Status::resource_exhausted("quota").into();
        assert_eq!(err.recovery(), Recovery::Retry);
    }

    #[test]
    fn missing_chat_triggers_re_resolution() {
        let err: YoutubeError = tonic::Status::not_found("liveChatId").into();
        assert_eq!(err.recovery(), Recovery::ReResolve);
    }

    #[test]
    fn an_offline_stream_stops_the_loop() {
        assert_eq!(YoutubeError::StreamOffline.recovery(), Recovery::Fatal);
    }

    #[test]
    fn http_status_codes_map_to_recovery() {
        let api = |status| YoutubeError::Api {
            status,
            message: String::new(),
        };
        assert_eq!(api(401).recovery(), Recovery::RefreshAuth);
        assert_eq!(api(403).recovery(), Recovery::RefreshAuth);
        assert_eq!(api(404).recovery(), Recovery::ReResolve);
        assert_eq!(api(429).recovery(), Recovery::Retry);
        assert_eq!(api(503).recovery(), Recovery::Retry);
        assert_eq!(api(400).recovery(), Recovery::Fatal);
    }

    #[test]
    fn redact_hides_credentials() {
        let out = redact("failed with Authorization: Bearer ya29.abc123 for user");
        assert!(!out.contains("ya29.abc123"), "{out}");
        assert!(out.contains("<redacted>"), "{out}");
    }

    #[test]
    fn redact_truncates_long_bodies() {
        let out = redact(&"x".repeat(5_000));
        assert!(out.len() <= 203, "len was {}", out.len());
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        // 'ç' is two bytes; truncating at 5 must not split it.
        let out = truncate("üçüncü mesaj", 5);
        assert!(out.ends_with("..."));
        assert!(out.is_char_boundary(out.len() - 3));
    }

    #[test]
    fn truncate_leaves_short_text_alone() {
        assert_eq!(truncate("short", 200), "short");
    }
}
