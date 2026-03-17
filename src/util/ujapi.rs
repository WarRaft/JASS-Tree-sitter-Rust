//! UjAPI integration — fetch latest release tag from GitHub, download
//! `uJAPIFiles/common.j`, and prepend `//<tag>` as the first line so the
//! local copy can be compared against the remote version.
//!
//! ## Fetching strategy
//!
//! * Only **one** GitHub request can be in-flight at a time.
//! * On failure the request is retried with **exponential backoff**
//!   (1 s → 2 s → 4 s → … → capped at 5 min).
//! * A successful fetch caches the result for the rest of the session.
//! * The fetch is triggered lazily the first time an `//import-ujapi!`
//!   directive is encountered during parsing.

use log::{error, info};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use url::Url;

// ─── Constants ───────────────────────────────────────────────────────────────

const GITHUB_API_LATEST: &str =
    "https://api.github.com/repos/UnryzeC/UjAPI/releases/latest";

/// Raw-content URL template.  `{tag}` is replaced with the release tag.
const RAW_URL_TEMPLATE: &str =
    "https://raw.githubusercontent.com/UnryzeC/UjAPI/{tag}/uJAPIFiles/common.j";

const USER_AGENT: &str = "JASS-Tree-sitter-Rust-LSP";

const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(5 * 60);

// ─── State ───────────────────────────────────────────────────────────────────

/// Result of the last successful fetch.
#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag: String,
    pub name: String,
    pub html_url: String,
    pub download_url: String,
}

#[derive(Debug)]
struct State {
    /// Cached result after a successful fetch.
    info: Option<ReleaseInfo>,
    /// `true` while a blocking HTTP call is running.
    in_flight: bool,
    /// Next allowed retry time (monotonic).
    next_retry: Option<Instant>,
    /// Current backoff duration (doubles on every failure).
    backoff: Duration,
}

impl Default for State {
    fn default() -> Self {
        Self {
            info: None,
            in_flight: false,
            next_retry: None,
            backoff: INITIAL_BACKOFF,
        }
    }
}

static STATE: Mutex<State> = Mutex::new(State {
    info: None,
    in_flight: false,
    next_retry: None,
    backoff: INITIAL_BACKOFF,
});

// ─── Public helpers ──────────────────────────────────────────────────────────

/// Return the cached latest release tag, or `None` if not fetched yet.
pub fn cached_release() -> Option<ReleaseInfo> {
    STATE.lock().ok()?.info.clone()
}

/// Read the UjAPI tag from the **first line** of a file on disk.
///
/// Expected format: `//<tag>` (e.g. `//v1.33.5`).
/// Returns `None` if the file doesn't exist or the first line doesn't match.
pub fn read_file_tag(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let first_line = content.lines().next()?.trim();
    first_line.strip_prefix("//").map(|s| s.to_string())
}

/// Resolve the relative `path` from the directive against `base_uri` and
/// return the absolute [`PathBuf`].
pub fn resolve_ujapi_path(base_uri: &Url, rel_path: &str) -> Option<PathBuf> {
    let base_path = base_uri.to_file_path().ok()?;
    let dir = base_path.parent()?;
    let norm = rel_path.replace('\\', "/");
    Some(dir.join(norm))
}

// ─── Fetch (with mutex + exponential backoff) ────────────────────────────────

/// Try to obtain the latest release info.
///
/// * If already cached — returns immediately.
/// * If another fetch is in-flight — returns `Err`.
/// * If the backoff timer hasn't elapsed — returns `Err`.
/// * Otherwise performs a **blocking** HTTP request (must be called from
///   `spawn_blocking`).
///
/// On failure the backoff is doubled (capped at [`MAX_BACKOFF`]).
/// On success the info is cached and backoff is reset.
pub fn fetch_latest_release() -> Result<ReleaseInfo, String> {
    // Fast path: already cached.
    {
        let st = STATE.lock().map_err(|e| e.to_string())?;
        if let Some(ref info) = st.info {
            return Ok(info.clone());
        }
    }

    // Try to acquire the "in-flight" slot.
    let backoff_copy;
    {
        let mut st = STATE.lock().map_err(|e| e.to_string())?;

        // Already fetched while we waited for the lock.
        if let Some(ref info) = st.info {
            return Ok(info.clone());
        }
        if st.in_flight {
            return Err("Another request is already in-flight".into());
        }
        if let Some(next) = st.next_retry {
            if Instant::now() < next {
                return Err(format!(
                    "Backoff: retry in {:.0}s",
                    (next - Instant::now()).as_secs_f64()
                ));
            }
        }
        st.in_flight = true;
        backoff_copy = st.backoff;
    }

    let result = do_fetch();

    // Update state.
    {
        let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
        st.in_flight = false;
        match &result {
            Ok(info) => {
                st.info = Some(info.clone());
                st.backoff = INITIAL_BACKOFF;
                st.next_retry = None;
            }
            Err(e) => {
                error!("ujapi fetch failed: {e}");
                let new_backoff = (backoff_copy * 2).min(MAX_BACKOFF);
                st.backoff = new_backoff;
                st.next_retry = Some(Instant::now() + new_backoff);
            }
        }
    }

    result
}

/// Force a fresh fetch, ignoring the cache.  Used when the user explicitly
/// requests a re-download.
pub fn force_fetch() -> Result<ReleaseInfo, String> {
    // Clear cache so fetch_latest_release actually does the request.
    {
        let mut st = STATE.lock().map_err(|e| e.to_string())?;
        st.info = None;
        st.next_retry = None;
        st.backoff = INITIAL_BACKOFF;
    }
    fetch_latest_release()
}

// ─── Download ────────────────────────────────────────────────────────────────

/// Download `uJAPIFiles/common.j` to `dest_path`.
///
/// The file is written with `//<tag>` as the **first line** so we can later
/// compare versions by reading just that line.
///
/// **Blocking** — must be called from `spawn_blocking`.
pub fn download_common_j(dest_path: &Path) -> Result<ReleaseInfo, String> {
    let release = force_fetch()?;

    info!(
        "ujapi: downloading {} → {}",
        release.download_url,
        dest_path.display()
    );

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    let resp = client
        .get(&release.download_url)
        .send()
        .map_err(|e| format!("Download failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Download HTTP {}", resp.status()));
    }

    let body = resp.text().map_err(|e| format!("Read body: {e}"))?;

    // Prepend //<tag> line.
    let content = format!("//{}\n{}", release.tag, body);

    // Create parent dirs.
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }

    std::fs::write(dest_path, content.as_bytes())
        .map_err(|e| format!("write: {e}"))?;

    info!(
        "ujapi: written {} bytes to {}",
        content.len(),
        dest_path.display()
    );

    Ok(release)
}

// ─── Internal ────────────────────────────────────────────────────────────────

fn do_fetch() -> Result<ReleaseInfo, String> {
    info!("ujapi: fetching latest release from {GITHUB_API_LATEST}");

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    let resp = client
        .get(GITHUB_API_LATEST)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| format!("GitHub API request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API HTTP {}", resp.status()));
    }

    let json: serde_json::Value =
        resp.json().map_err(|e| format!("JSON parse: {e}"))?;

    let tag = json["tag_name"]
        .as_str()
        .ok_or("Missing tag_name")?
        .to_string();

    let name = json["name"]
        .as_str()
        .unwrap_or(&tag)
        .to_string();

    let html_url = json["html_url"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let download_url = RAW_URL_TEMPLATE.replace("{tag}", &tag);

    let info = ReleaseInfo {
        tag,
        name,
        html_url,
        download_url,
    };

    info!("ujapi: latest release = {} ({})", info.tag, info.name);

    Ok(info)
}
