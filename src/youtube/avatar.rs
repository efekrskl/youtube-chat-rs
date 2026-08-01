use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use image::ImageReader;
use image::imageops::FilterType;
use log::debug;
use reqwest::Url;

use crate::app::event::KittyAvatar;

const AVATAR_PX: u32 = 32;
const AVATAR_COLS: u16 = 2;
/// Profile pictures are tiny; anything larger is not something we should decode.
const MAX_AVATAR_BYTES: u64 = 2 * 1024 * 1024;
/// Bound on how many avatars we keep on disk / in memory for one session.
const MAX_CACHED_AVATARS: usize = 512;
/// A busy stream introduces new authors faster than the CDN answers; without a
/// ceiling we would spawn hundreds of concurrent requests during a burst.
const MAX_CONCURRENT_FETCHES: usize = 8;

/// Hosts YouTube serves profile pictures from. The URL comes from the API
/// response, so it is attacker-influenced data: restrict it rather than
/// fetching whatever we are handed.
const ALLOWED_AVATAR_HOSTS: &[&str] = &["ggpht.com", "googleusercontent.com", "ytimg.com"];

pub fn is_allowed_avatar_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };

    if parsed.scheme() != "https" {
        return false;
    }

    let Some(host) = parsed.host_str() else {
        return false;
    };

    ALLOWED_AVATAR_HOSTS
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Pending,
    Failed,
}

struct Inner {
    ready: HashMap<String, Arc<KittyAvatar>>,
    /// URLs currently being fetched or known to be unusable, so we never issue
    /// the same request twice.
    inflight: HashMap<String, Slot>,
    order: Vec<String>,
    dir: PathBuf,
}

/// Downloads and caches Kitty-renderable avatars. Every method is cheap and
/// non-blocking; the actual work happens on a caller-spawned task so the chat
/// stream is never held up by a slow CDN.
#[derive(Clone)]
pub struct AvatarService {
    /// Deliberately a separate client from the API one: this client must never
    /// carry the OAuth bearer token, because the destination host comes from
    /// the API payload.
    http: reqwest::Client,
    inner: Arc<Mutex<Inner>>,
}

impl AvatarService {
    pub fn new(http: reqwest::Client, dir: PathBuf) -> Self {
        Self {
            http,
            inner: Arc::new(Mutex::new(Inner {
                ready: HashMap::new(),
                inflight: HashMap::new(),
                order: Vec::new(),
                dir,
            })),
        }
    }

    pub fn cached(&self, url: &str) -> Option<Arc<KittyAvatar>> {
        self.inner.lock().ok()?.ready.get(url).cloned()
    }

    /// Claim the right to fetch `url`. Returns false when it is already cached,
    /// already being fetched, known to be broken, or when too many fetches are
    /// already outstanding.
    ///
    /// Skipping a claim only costs one missing avatar: the next message from
    /// the same author will try again.
    pub fn claim(&self, url: &str) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };

        if inner.ready.contains_key(url) || inner.inflight.contains_key(url) {
            return false;
        }
        if inner.ready.len() >= MAX_CACHED_AVATARS {
            return false;
        }

        let pending = inner
            .inflight
            .values()
            .filter(|slot| **slot == Slot::Pending)
            .count();
        if pending >= MAX_CONCURRENT_FETCHES {
            return false;
        }

        inner.inflight.insert(url.to_string(), Slot::Pending);
        true
    }

    /// Download, decode and store an avatar. Only call after a successful
    /// `claim`.
    pub async fn fetch(&self, url: &str) -> Option<Arc<KittyAvatar>> {
        let avatar = self.fetch_inner(url).await;

        let Ok(mut inner) = self.inner.lock() else {
            return None;
        };

        match &avatar {
            Some(avatar) => {
                inner.inflight.remove(url);
                inner.ready.insert(url.to_string(), avatar.clone());
                inner.order.push(url.to_string());
            }
            None => {
                // Remember the failure so a chatty author does not make us
                // retry a broken URL on every single message.
                inner.inflight.insert(url.to_string(), Slot::Failed);
            }
        }

        avatar
    }

    async fn fetch_inner(&self, url: &str) -> Option<Arc<KittyAvatar>> {
        if !is_allowed_avatar_url(url) {
            debug!("refusing avatar from disallowed url");
            return None;
        }

        let response = self.http.get(url).send().await.ok()?;
        if !response.status().is_success() {
            debug!("avatar fetch failed with status {}", response.status());
            return None;
        }

        // Reject oversized payloads before buffering them.
        if let Some(len) = response.content_length()
            && len > MAX_AVATAR_BYTES
        {
            debug!("avatar too large ({len} bytes)");
            return None;
        }

        let bytes = response.bytes().await.ok()?;
        if bytes.len() as u64 > MAX_AVATAR_BYTES {
            debug!("avatar body too large ({} bytes)", bytes.len());
            return None;
        }

        let path = self.avatar_path(url)?;
        let id = avatar_id(url);

        // Decoding, resizing and writing to disk are all blocking work; keeping
        // them on a runtime worker would stall the gRPC reader.
        tokio::task::spawn_blocking(move || {
            let mut reader = ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .ok()?;

            // Guard against decompression bombs from a compromised CDN response.
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(4096);
            limits.max_image_height = Some(4096);
            limits.max_alloc = Some(64 * 1024 * 1024);
            reader.limits(limits);

            let image = reader.decode().ok()?;
            let rgba = image
                .resize_to_fill(AVATAR_PX, AVATAR_PX, FilterType::Triangle)
                .to_rgba8();
            let (width, height) = rgba.dimensions();

            std::fs::write(&path, rgba.as_raw()).ok()?;

            Some(Arc::new(KittyAvatar {
                id,
                cols: AVATAR_COLS,
                width,
                height,
                path: path.to_string_lossy().into_owned(),
            }))
        })
        .await
        .ok()
        .flatten()
    }

    fn avatar_path(&self, url: &str) -> Option<PathBuf> {
        let inner = self.inner.lock().ok()?;
        // Full 64-bit hash: the previous 24-bit id collided often enough to
        // show the wrong person's avatar.
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        Some(inner.dir.join(format!("{:016x}.rgba", hasher.finish())))
    }
}

/// Kitty image ids are 24-bit; collisions there only affect which placeholder
/// colour is drawn, not which file is loaded.
fn avatar_id(url: &str) -> u32 {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    // Kitty treats id 0 as "assign one for me", so keep it non-zero.
    ((hasher.finish() as u32) & 0x00FF_FFFF).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_youtube_avatar_hosts() {
        assert!(is_allowed_avatar_url("https://yt3.ggpht.com/ytc/abc=s32"));
        assert!(is_allowed_avatar_url(
            "https://lh3.googleusercontent.com/a/abc"
        ));
        assert!(is_allowed_avatar_url("https://ggpht.com/a"));
    }

    #[test]
    fn rejects_other_hosts_and_schemes() {
        assert!(!is_allowed_avatar_url("https://evil.example.com/a.png"));
        assert!(!is_allowed_avatar_url("http://yt3.ggpht.com/a.png"));
        assert!(!is_allowed_avatar_url("file:///etc/passwd"));
        assert!(!is_allowed_avatar_url("not a url"));
        // Suffix must be a real domain boundary, not a substring match.
        assert!(!is_allowed_avatar_url("https://evilggpht.com/a.png"));
        assert!(!is_allowed_avatar_url("https://ggpht.com.evil.net/a.png"));
    }

    #[test]
    fn avatar_ids_are_non_zero_and_stable() {
        let a = avatar_id("https://yt3.ggpht.com/a");
        assert_eq!(a, avatar_id("https://yt3.ggpht.com/a"));
        assert_ne!(a, 0);
        assert!(a <= 0x00FF_FFFF);
    }

    #[test]
    fn claim_is_granted_once_per_url() {
        let service = AvatarService::new(reqwest::Client::new(), std::env::temp_dir());
        assert!(service.claim("https://yt3.ggpht.com/a"));
        assert!(!service.claim("https://yt3.ggpht.com/a"));
        assert!(service.claim("https://yt3.ggpht.com/b"));
    }

    // A burst of new authors must not spawn an unbounded number of requests.
    #[test]
    fn claim_caps_concurrent_fetches() {
        let service = AvatarService::new(reqwest::Client::new(), std::env::temp_dir());
        for i in 0..MAX_CONCURRENT_FETCHES {
            assert!(service.claim(&format!("https://yt3.ggpht.com/{i}")), "{i}");
        }
        assert!(!service.claim("https://yt3.ggpht.com/one-too-many"));
    }
}
