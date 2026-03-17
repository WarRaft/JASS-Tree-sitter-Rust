//! Unified on-disk cache — **one file = one cache entry**.
//!
//! Replaces the old `symbol_cache` + `ref_cache` split.  Each source file's
//! parse output is stored as a single bitcode blob keyed by SHA-256(URI).
//!
//! ## Stored data
//!
//! * [`FileMeta`] — `(size, mtime)` for cheap `stat()`-based staleness checks.
//! * `content_hash` — SHA-256 of file content at parse time.
//! * [`FileSymbols`] — exported symbols (functions, natives, globals, types).
//! * [`RefMap`] — all references (highlight, definition, rename).
//! * `func_decl_keys` — DeclKeys of function/native declarations (needed by
//!   `find_decl_key_by_name`).
//!
//! Cache directory: `$CACHE_DIR/jass-tree-sitter-cache/`

use crate::lng::jass::symbol::FileSymbols;
use crate::lsp::ref_map::{DeclKey, RefMap};
use log::{error, info};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use url::Url;

// ─── Helpers ─────────────────────────────────────────────────────────────────

const CACHE_DIR_NAME: &str = "jass-tree-sitter-cache";

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

/// Lightweight file metadata used for staleness checks.
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

// ─── Content hash ────────────────────────────────────────────────────────────

/// Compute a SHA-256 hash of rope content (used as invalidation key).
pub fn content_hash(rope: &lapce_xi_rope::Rope) -> [u8; 32] {
    let mut hasher = Sha256::new();
    let text = rope.slice_to_cow(0..rope.len());
    hasher.update(text.as_bytes());
    hasher.finalize().into()
}

// ─── On-disk format ──────────────────────────────────────────────────────────

/// What gets stored on disk — everything needed to reconstruct a partial
/// [`ParseSnapshot`] without re-reading the source file.
#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    /// Original URI string — needed for `load_all` to map filenames back.
    uri: String,
    /// Metadata at the time the cache was written.
    meta: FileMeta,
    /// SHA-256 of file content at the time the cache was written.
    content_hash: [u8; 32],
    /// Exported symbols.
    symbols: FileSymbols,
    /// Reference map (highlight, definition, rename).
    ref_map: RefMap,
    /// DeclKeys of function/native declarations.
    func_decl_keys: HashSet<DeclKey>,
}

/// Result of loading a cache entry.
pub struct CacheData {
    pub meta: FileMeta,
    pub content_hash: [u8; 32],
    pub symbols: FileSymbols,
    pub ref_map: RefMap,
    pub func_decl_keys: HashSet<DeclKey>,
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Try to load the cached entry for `uri`.
///
/// Returns the full `CacheData` if the cache file exists.
/// The caller must compare `meta` with the current file metadata to decide
/// whether the entry is stale.
pub fn load(uri: &Url) -> Option<CacheData> {
    let path = cache_file(uri)?;
    if !path.exists() {
        return None;
    }

    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            error!("file_cache: read {:?}: {}", path, e);
            return None;
        }
    };

    let entry: CacheEntry = match bitcode::deserialize(&data) {
        Ok(e) => e,
        Err(e) => {
            error!("file_cache: deserialize {:?}: {}", path, e);
            let _ = fs::remove_file(&path);
            return None;
        }
    };

    Some(CacheData {
        meta: entry.meta,
        content_hash: entry.content_hash,
        symbols: entry.symbols,
        ref_map: entry.ref_map,
        func_decl_keys: entry.func_decl_keys,
    })
}

/// Store a file's parse output to disk.
pub fn store(
    uri: &Url,
    meta: FileMeta,
    content_hash: [u8; 32],
    symbols: &FileSymbols,
    ref_map: &RefMap,
    func_decl_keys: &HashSet<DeclKey>,
) {
    let Some(path) = cache_file(uri) else { return };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // NOTE: CacheEntryRef can't derive bitcode::Encode (lifetimes), so build owned.
    let entry = CacheEntry {
        uri: uri.as_str().to_string(),
        meta,
        content_hash,
        symbols: symbols.clone(),
        ref_map: ref_map.clone(),
        func_decl_keys: func_decl_keys.clone(),
    };

    match bitcode::serialize(&entry) {
        Ok(data) => {
            if let Err(e) = fs::write(&path, data) {
                error!("file_cache: write {:?}: {}", path, e);
            }
        }
        Err(e) => error!("file_cache: serialize {:?}: {}", path, e),
    }
}

/// Load **all** cached entries from the cache directory.
///
/// Returns `(uri, CacheData)` for every valid `.bin` file.
/// Corrupted entries are deleted silently.
pub fn load_all() -> Vec<(Url, CacheData)> {
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
        match bitcode::deserialize::<CacheEntry>(&data) {
            Ok(ce) => {
                if let Ok(uri) = Url::parse(&ce.uri) {
                    result.push((
                        uri,
                        CacheData {
                            meta: ce.meta,
                            content_hash: ce.content_hash,
                            symbols: ce.symbols,
                            ref_map: ce.ref_map,
                            func_decl_keys: ce.func_decl_keys,
                        },
                    ));
                }
            }
            Err(_) => {
                let _ = fs::remove_file(&path);
            }
        }
    }
    result
}

/// Delete **all** cache files — used by the forced rescan command.
pub fn purge_all() {
    let Some(dir) = cache_dir() else { return };
    if !dir.exists() {
        return;
    }
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut removed = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("bin") {
            let _ = fs::remove_file(&path);
            removed += 1;
        }
    }
    info!("file_cache: purge_all removed {} entries", removed);
}

/// Remove cache files for all URIs **not** in `keep`.
///
/// Call on startup after loading the import graph to garbage-collect
/// entries for files no longer in the project.
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
        info!("file_cache: gc removed {} stale entries", removed);
    }
}

/// Try to load the cached entry for `uri` only if it's fresh (stat-based).
///
/// Compares the stored `FileMeta` (size + mtime) against the current file.
/// Returns `Some(CacheData)` without reading the source file content.
pub fn load_if_fresh(uri: &Url) -> Option<CacheData> {
    let cached = load(uri)?;
    let current_meta = FileMeta::from_uri(uri)?;
    if current_meta != cached.meta {
        return None;
    }
    Some(cached)
}

