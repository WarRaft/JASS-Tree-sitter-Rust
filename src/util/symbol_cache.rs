//! Persistent on-disk cache for [`FileSymbols`].
//!
//! Each file's symbols are stored as a bincode blob keyed by SHA-256(URI).
//! Alongside the payload we store the file's **mtime** (seconds since epoch)
//! and **size** (bytes) so that stale entries are detected without reading the
//! full file content.
//!
//! Cache directory: `$CACHE_DIR/jass-tree-sitter-symbols/`

use crate::lng::jass::symbol::FileSymbols;
use log::{error, info};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use url::Url;

// ─── Helpers ─────────────────────────────────────────────────────────────────

const CACHE_DIR_NAME: &str = "jass-tree-sitter-symbols";

fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join(CACHE_DIR_NAME))
}

fn uri_to_filename(uri: &Url) -> String {
    let mut hasher = Sha256::new();
    hasher.update(uri.as_str().as_bytes());
    let hash = hasher.finalize();
    format!("{:x}.bin", hash)
}

fn cache_file(uri: &Url) -> Option<PathBuf> {
    cache_dir().map(|d| d.join(uri_to_filename(uri)))
}

// ─── File metadata ───────────────────────────────────────────────────────────

/// Lightweight file metadata used for staleness check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileMeta {
    /// File size in bytes.
    pub size: u64,
    /// Modification time — seconds since UNIX epoch.
    pub mtime_secs: u64,
}

impl FileMeta {
    /// Read metadata from a filesystem path.
    pub fn from_path(path: &std::path::Path) -> Option<Self> {
        let md = fs::metadata(path).ok()?;
        let mtime_secs = md
            .modified()
            .ok()?
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()?
            .as_secs();
        Some(Self {
            size: md.len(),
            mtime_secs,
        })
    }

    /// Read metadata for a `file://` URI.
    pub fn from_uri(uri: &Url) -> Option<Self> {
        let path = uri.to_file_path().ok()?;
        Self::from_path(&path)
    }
}

// ─── On-disk format ──────────────────────────────────────────────────────────

/// What gets stored on disk.
#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    /// Original URI string — needed for `load_all` to map filenames back.
    uri: String,
    /// Metadata at the time the cache was written.
    meta: FileMeta,
    /// SHA-256 of file content at the time the cache was written.
    /// Stored so consumers (scope resolver) can use the hash without
    /// re-reading the file from disk.
    content_hash: [u8; 32],
    /// The actual symbol data.
    symbols: FileSymbols,
}

/// Borrowing variant for serialization without cloning.
#[derive(serde::Serialize)]
struct CacheEntryRef<'a> {
    uri: &'a str,
    meta: FileMeta,
    content_hash: [u8; 32],
    symbols: &'a FileSymbols,
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Try to load cached symbols for `uri`.
///
/// Returns `Some((meta, content_hash, symbols))` only if the cache file exists.
/// The caller must compare `meta` with the current file metadata to decide
/// whether the entry is stale.
pub fn load(uri: &Url) -> Option<(FileMeta, [u8; 32], FileSymbols)> {
    let path = cache_file(uri)?;
    if !path.exists() {
        return None;
    }

    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            error!("symbol_cache: read {:?}: {}", path, e);
            return None;
        }
    };

    let entry: CacheEntry = match bincode::deserialize(&data) {
        Ok(e) => e,
        Err(e) => {
            error!("symbol_cache: deserialize {:?}: {}", path, e);
            let _ = fs::remove_file(&path);
            return None;
        }
    };

    Some((entry.meta, entry.content_hash, entry.symbols))
}

/// Store symbols to disk for `uri` with the given file metadata and content hash.
pub fn store(uri: &Url, meta: FileMeta, content_hash: [u8; 32], symbols: &FileSymbols) {
    let Some(path) = cache_file(uri) else { return };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let entry = CacheEntryRef {
        uri: uri.as_str(),
        meta,
        content_hash,
        symbols,
    };

    match bincode::serialize(&entry) {
        Ok(data) => {
            if let Err(e) = fs::write(&path, data) {
                error!("symbol_cache: write {:?}: {}", path, e);
            }
        }
        Err(e) => error!("symbol_cache: serialize {:?}: {}", path, e),
    }
}

/// Remove the cache file for a single URI.
#[allow(dead_code)]
pub fn evict(uri: &Url) {
    if let Some(path) = cache_file(uri) {
        if path.exists() {
            let _ = fs::remove_file(&path);
            info!("symbol_cache: evicted {}", uri);
        }
    }
}

/// Load **all** cached entries from the cache directory.
///
/// Returns `(uri, meta, content_hash, symbols)` for every valid `.bin` file.
/// Corrupted entries are deleted silently.
pub fn load_all() -> Vec<(Url, FileMeta, [u8; 32], FileSymbols)> {
    let Some(dir) = cache_dir() else {
        return vec![];
    };
    if !dir.exists() {
        return vec![];
    }

    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    let mut result = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("bin") {
            continue;
        }
        let data = match fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        match bincode::deserialize::<CacheEntry>(&data) {
            Ok(ce) => {
                if let Ok(uri) = Url::parse(&ce.uri) {
                    result.push((uri, ce.meta, ce.content_hash, ce.symbols));
                }
            }
            Err(_) => {
                let _ = fs::remove_file(&path);
            }
        }
    }
    result
}

/// Remove cache files for all URIs **not** in `keep`.
pub fn gc(keep: &HashSet<String>) {
    let Some(dir) = cache_dir() else { return };
    if !dir.exists() {
        return;
    }

    let keep_filenames: HashSet<String> = keep
        .iter()
        .map(|uri_str| {
            let mut hasher = Sha256::new();
            hasher.update(uri_str.as_bytes());
            let hash = hasher.finalize();
            format!("{:x}.bin", hash)
        })
        .collect();

    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut removed = 0usize;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".bin") && !keep_filenames.contains(&name) {
            let _ = fs::remove_file(entry.path());
            removed += 1;
        }
    }

    if removed > 0 {
        info!("symbol_cache: gc removed {} stale entries", removed);
    }
}

