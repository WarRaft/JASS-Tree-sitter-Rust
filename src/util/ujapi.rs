//! UjAPI integration — fetch latest release tag from GitHub, download
//! `uJAPIFiles/common.j`, and prepend `//<tag>` as the first line so the
//! local copy can be compared against the remote version.
//!
//! ## Caching strategy
//!
//! * The last successfully fetched release info is **persisted to disk**
//!   (`$CACHE_DIR/jass-tree-sitter-cache/ujapi-release.json`).
//! * On LSP startup the cached info is loaded from disk — **no network**.
//! * A **single background check** is triggered lazily the first time an
//!   `//import-ujapi!` directive is encountered.  On failure it retries
//!   with exponential backoff (1 s → 2 s → 4 s → … capped at 5 min),
//!   up to [`MAX_BG_ATTEMPTS`] total attempts.
//! * Only **one** HTTP request can be in-flight at a time.

use log::{error, info, warn};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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

const CACHE_DIR_NAME: &str = "jass-tree-sitter-cache";
const CACHE_FILE_NAME: &str = "ujapi-release.json";

const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(5 * 60);
/// Stop retrying after this many failed background attempts.
const MAX_BG_ATTEMPTS: u32 = 5;

// ─── State ───────────────────────────────────────────────────────────────────

/// Result of the last successful fetch.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReleaseInfo {
    pub tag: String,
    pub name: String,
    pub html_url: String,
    pub download_url: String,
}

#[derive(Debug)]
struct State {
    /// Cached result (loaded from disk or from a successful fetch).
    info: Option<ReleaseInfo>,
    /// `true` while a blocking HTTP call is running.
    in_flight: bool,
    /// Next allowed background retry time (monotonic).
    next_retry: Option<Instant>,
    /// Current backoff duration (doubles on every failure).
    backoff: Duration,
}

static STATE: Mutex<State> = Mutex::new(State {
    info: None,
    in_flight: false,
    next_retry: None,
    backoff: INITIAL_BACKOFF,
});

/// Whether the disk cache has been loaded.
static DISK_LOADED: AtomicBool = AtomicBool::new(false);
/// Whether a successful background check has completed this session.
static BG_DONE: AtomicBool = AtomicBool::new(false);
/// Number of background attempts made this session.
static BG_ATTEMPTS: AtomicU32 = AtomicU32::new(0);

// ─── Disk cache ──────────────────────────────────────────────────────────────

fn cache_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join(CACHE_DIR_NAME).join(CACHE_FILE_NAME))
}

fn load_from_disk() -> Option<ReleaseInfo> {
    let path = cache_path()?;
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_to_disk(info: &ReleaseInfo) {
    let Some(path) = cache_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(info) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                error!("ujapi: write cache: {e}");
            }
        }
        Err(e) => error!("ujapi: serialize cache: {e}"),
    }
}

/// Ensure the disk cache has been loaded into memory (idempotent).
fn ensure_disk_loaded() {
    if DISK_LOADED.swap(true, Ordering::SeqCst) {
        return; // already loaded
    }
    if let Some(info) = load_from_disk() {
        info!("ujapi: loaded cached release {} from disk", info.tag);
        let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if st.info.is_none() {
            st.info = Some(info);
        }
    }
}

// ─── Public helpers ──────────────────────────────────────────────────────────

/// Return the cached latest release tag, or `None` if not fetched yet.
pub fn cached_release() -> Option<ReleaseInfo> {
    ensure_disk_loaded();
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

// ─── Background check (lazy, once per session) ──────────────────────────────

/// Schedule a **non-blocking** background version check.
///
/// Called from the parse thread when `//import-ujapi!` is encountered.
/// Does nothing if:
/// - a check already succeeded this session,
/// - the retry limit has been reached,
/// - a request is already in-flight,
/// - the backoff timer hasn't elapsed.
///
/// The actual HTTP request runs on `spawn_blocking`.
pub fn schedule_background_check() {
    // Already succeeded this session — nothing to do.
    if BG_DONE.load(Ordering::Relaxed) {
        return;
    }
    // Exhausted retries.
    if BG_ATTEMPTS.load(Ordering::Relaxed) >= MAX_BG_ATTEMPTS {
        return;
    }

    // Check timing / in-flight under lock.
    {
        let st = match STATE.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        if st.in_flight {
            return;
        }
        if let Some(next) = st.next_retry {
            if Instant::now() < next {
                return;
            }
        }
    }

    // Fire and forget.
    std::thread::spawn(|| {
        let attempt = BG_ATTEMPTS.fetch_add(1, Ordering::Relaxed) + 1;
        info!("ujapi: background check attempt {}/{}", attempt, MAX_BG_ATTEMPTS);
        match fetch_latest_release() {
            Ok(rel) => {
                BG_DONE.store(true, Ordering::Relaxed);
                info!("ujapi: background check OK — {}", rel.tag);
                // Trigger a diagnostics refresh so Hint updates.
                tokio::runtime::Handle::try_current().ok().map(|h| {
                    h.spawn(async {
                        crate::util::file_store::send_refresh_all().await;
                    });
                });
            }
            Err(e) => {
                warn!("ujapi: background check failed ({}/{}): {}", attempt, MAX_BG_ATTEMPTS, e);
            }
        }
    });
}

// ─── Fetch ───────────────────────────────────────────────────────────────────

/// Fetch the latest release info from GitHub.
///
/// * If another fetch is in-flight — returns `Err`.
/// * If the backoff timer hasn't elapsed — returns `Err`.
/// * Otherwise performs a **blocking** HTTP request.
///
/// On success the info is cached in memory **and** persisted to disk.
/// On failure the backoff is doubled (capped at [`MAX_BACKOFF`]).
pub fn fetch_latest_release() -> Result<ReleaseInfo, String> {
    ensure_disk_loaded();

    // Try to acquire the "in-flight" slot.
    let backoff_copy;
    {
        let mut st = STATE.lock().map_err(|e| e.to_string())?;
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
                save_to_disk(info);
            }
            Err(_) => {
                let new_backoff = (backoff_copy * 2).min(MAX_BACKOFF);
                st.backoff = new_backoff;
                st.next_retry = Some(Instant::now() + new_backoff);
            }
        }
    }

    result
}

// ─── Download ────────────────────────────────────────────────────────────────

/// Download `uJAPIFiles/common.j` to `dest_path`.
///
/// **Always** fetches from GitHub (ignores backoff / attempt limits) because
/// this is an explicit user action.
///
/// **Blocking** — must be called from `spawn_blocking`.
pub fn download_common_j(dest_path: &Path) -> Result<ReleaseInfo, String> {
    // Reset limits — user explicitly asked.
    {
        let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
        st.next_retry = None;
        st.backoff = INITIAL_BACKOFF;
        st.in_flight = false; // force-release in case a bg check hung
    }
    BG_ATTEMPTS.store(0, Ordering::Relaxed);
    BG_DONE.store(false, Ordering::Relaxed);

    let release = fetch_latest_release()?;

    info!(
        "ujapi: downloading {} → {}",
        release.download_url,
        dest_path.display()
    );

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
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

    BG_DONE.store(true, Ordering::Relaxed);
    Ok(release)
}

// ─── Internal ────────────────────────────────────────────────────────────────

fn do_fetch() -> Result<ReleaseInfo, String> {
    info!("ujapi: fetching latest release from {GITHUB_API_LATEST}");

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(15))
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
