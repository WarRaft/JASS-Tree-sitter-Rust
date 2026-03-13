//! Persistent on-disk cache for [`RefMap`].
//!
//! Each file's `RefMap` is stored as a bincode blob keyed by a SHA-256 hash
//! of the file URI.  A content hash is stored alongside the payload so that
//! stale entries (file changed on disk) are automatically invalidated.
//!
//! Cache directory: `$CACHE_DIR/jass-tree-sitter-refs/`

use crate::lsp::ref_map::RefMap;
use log::{error, info};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use url::Url;

// ─── Helpers ─────────────────────────────────────────────────────────────────

const CACHE_DIR_NAME: &str = "jass-tree-sitter-refs";

/// Root directory for all RefMap cache files.
fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join(CACHE_DIR_NAME))
}

/// Deterministic filename for a given URI: `sha256(uri_string).bin`.
fn uri_to_filename(uri: &Url) -> String {
    let mut hasher = Sha256::new();
    hasher.update(uri.as_str().as_bytes());
    let hash = hasher.finalize();
    format!("{:x}.bin", hash)
}

/// Full path to the cache file for a URI.
fn cache_file(uri: &Url) -> Option<PathBuf> {
    cache_dir().map(|d| d.join(uri_to_filename(uri)))
}

// ─── On-disk format ──────────────────────────────────────────────────────────

/// Wrapper stored on disk: content hash + serialized RefMap.
#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    /// SHA-256 of the file *content* at the time RefMap was built.
    content_hash: [u8; 32],
    /// The actual reference data.
    ref_map: RefMap,
}

/// Borrowing variant for serialization without cloning.
#[derive(serde::Serialize)]
struct CacheEntryRef<'a> {
    content_hash: [u8; 32],
    ref_map: &'a RefMap,
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Compute a SHA-256 hash of rope content (used as invalidation key).
pub fn content_hash(rope: &lapce_xi_rope::Rope) -> [u8; 32] {
    let mut hasher = Sha256::new();
    let text = rope.slice_to_cow(0..rope.len());
    hasher.update(text.as_bytes());
    hasher.finalize().into()
}

/// Try to load a cached `RefMap` for `uri`.
///
/// Returns `Some(ref_map)` only if a cache file exists **and** the stored
/// content hash matches `current_hash` (i.e. the file hasn't changed).
pub fn load(uri: &Url, current_hash: &[u8; 32]) -> Option<RefMap> {
    let path = cache_file(uri)?;
    if !path.exists() {
        return None;
    }

    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            error!("ref_cache: read {:?}: {}", path, e);
            return None;
        }
    };

    let entry: CacheEntry = match bincode::deserialize(&data) {
        Ok(e) => e,
        Err(e) => {
            error!("ref_cache: deserialize {:?}: {}", path, e);
            // Corrupted — remove it.
            let _ = fs::remove_file(&path);
            return None;
        }
    };

    if &entry.content_hash != current_hash {
        // Content changed since cache was written — stale.
        return None;
    }

    Some(entry.ref_map)
}

/// Store a `RefMap` to disk for `uri` with the given content hash.
pub fn store(uri: &Url, current_hash: &[u8; 32], ref_map: &RefMap) {
    let Some(path) = cache_file(uri) else { return };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let entry = CacheEntryRef {
        content_hash: *current_hash,
        ref_map,
    };

    match bincode::serialize(&entry) {
        Ok(data) => {
            if let Err(e) = fs::write(&path, data) {
                error!("ref_cache: write {:?}: {}", path, e);
            }
        }
        Err(e) => error!("ref_cache: serialize {:?}: {}", path, e),
    }
}

/// Remove the cache file for a single URI.
pub fn evict(uri: &Url) {
    if let Some(path) = cache_file(uri) {
        if path.exists() {
            let _ = fs::remove_file(&path);
            info!("ref_cache: evicted {}", uri);
        }
    }
}

/// Remove cache files for all URIs **not** in `keep`.
///
/// Call this on startup after loading the import graph to garbage-collect
/// cache entries for files that no longer exist in the project.
pub fn gc(keep: &HashSet<String>) {
    let Some(dir) = cache_dir() else { return };
    if !dir.exists() {
        return;
    }

    // Build the set of filenames that should be kept.
    let keep_filenames: HashSet<String> = keep.iter().map(|uri_str| {
        let mut hasher = Sha256::new();
        hasher.update(uri_str.as_bytes());
        let hash = hasher.finalize();
        format!("{:x}.bin", hash)
    }).collect();

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
        info!("ref_cache: gc removed {} stale entries", removed);
    }
}



