//! Unified on-disk cache backed by [`redb`].
//!
//! Each source file's parse output is stored as a single bitcode blob
//! keyed by the file's URI string.
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
//! Database table: `file_cache` in the shared `redb` database.

use crate::lng::jass::symbol::FileSymbols;
use crate::http::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::http::ref_map::{DeclKey, RefMap};
use crate::util::cache_db;
use log::{error, info};
use redb::{ReadableDatabase, ReadableTable};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::time::SystemTime;
use url::Url;

// ─── File metadata ───────────────────────────────────────────────────────────

/// Count `(errors, warnings)` in a diagnostics slice.
pub fn diag_counts(diags: &[Diagnostic]) -> (u32, u32) {
    let mut errors = 0u32;
    let mut warnings = 0u32;
    for d in diags {
        match d.severity {
            Some(DiagnosticSeverity::Error) => errors += 1,
            Some(DiagnosticSeverity::Warning) => warnings += 1,
            _ => {}
        }
    }
    (errors, warnings)
}

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
    /// DeclKeys of variable declarations (globals + locals).
    #[serde(default)]
    var_decl_keys: HashSet<DeclKey>,
    /// DeclKeys of function parameter declarations.
    #[serde(default)]
    arg_decl_keys: HashSet<DeclKey>,
    /// Number of Error-severity diagnostics at parse time.
    #[serde(default)]
    diag_errors: u32,
    /// Number of Warning-severity diagnostics at parse time.
    #[serde(default)]
    diag_warnings: u32,
}

/// Result of loading a cache entry.
#[allow(dead_code)]
pub struct CacheData {
    pub meta: FileMeta,
    pub content_hash: [u8; 32],
    pub symbols: FileSymbols,
    pub ref_map: RefMap,
    pub func_decl_keys: HashSet<DeclKey>,
    pub var_decl_keys: HashSet<DeclKey>,
    pub arg_decl_keys: HashSet<DeclKey>,
    pub diag_errors: u32,
    pub diag_warnings: u32,
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Try to load the cached entry for `uri`.
///
/// Returns the full `CacheData` if the entry exists in the database.
/// The caller must compare `meta` with the current file metadata to decide
/// whether the entry is stale.
pub fn load(uri: &Url) -> Option<CacheData> {
    let db = cache_db::db()?;
    let read_txn = db.begin_read().ok()?;
    let table = read_txn.open_table(cache_db::FILE_CACHE_TABLE).ok()?;
    let guard = table.get(uri.as_str()).ok()??;
    let bytes: &[u8] = guard.value();

    let entry: CacheEntry = match bitcode::deserialize(bytes) {
        Ok(e) => e,
        Err(e) => {
            error!("file_cache: deserialize {}: {}", uri, e);
            // Remove corrupted entry.
            drop(guard);
            drop(table);
            drop(read_txn);
            remove_entry(uri);
            return None;
        }
    };

    Some(CacheData {
        meta: entry.meta,
        content_hash: entry.content_hash,
        symbols: entry.symbols,
        ref_map: entry.ref_map,
        func_decl_keys: entry.func_decl_keys,
        var_decl_keys: entry.var_decl_keys,
        arg_decl_keys: entry.arg_decl_keys,
        diag_errors: entry.diag_errors,
        diag_warnings: entry.diag_warnings,
    })
}

/// Store a file's parse output to the database.
pub fn store(
    uri: &Url,
    meta: FileMeta,
    content_hash: [u8; 32],
    symbols: &FileSymbols,
    ref_map: &RefMap,
    func_decl_keys: &HashSet<DeclKey>,
    var_decl_keys: &HashSet<DeclKey>,
    arg_decl_keys: &HashSet<DeclKey>,
    diag_errors: u32,
    diag_warnings: u32,
) {
    let Some(db) = cache_db::db() else { return };

    let entry = CacheEntry {
        meta,
        content_hash,
        symbols: symbols.clone(),
        ref_map: ref_map.clone(),
        func_decl_keys: func_decl_keys.clone(),
        var_decl_keys: var_decl_keys.clone(),
        arg_decl_keys: arg_decl_keys.clone(),
        diag_errors,
        diag_warnings,
    };

    let data = match bitcode::serialize(&entry) {
        Ok(d) => d,
        Err(e) => {
            error!("file_cache: serialize {}: {}", uri, e);
            return;
        }
    };

    let write_txn = match db.begin_write() {
        Ok(t) => t,
        Err(e) => {
            error!("file_cache: begin_write: {}", e);
            return;
        }
    };
    {
        let mut table = match write_txn.open_table(cache_db::FILE_CACHE_TABLE) {
            Ok(t) => t,
            Err(e) => {
                error!("file_cache: open table: {}", e);
                return;
            }
        };
        if let Err(e) = table.insert(uri.as_str(), data.as_slice()) {
            error!("file_cache: insert {}: {}", uri, e);
            return;
        }
    }
    if let Err(e) = write_txn.commit() {
        error!("file_cache: commit: {}", e);
    }
}


/// Diagnostic summary — lightweight counts for file decoration badges.
pub struct DiagSummary {
    pub uri: String,
    pub errors: u32,
    pub warnings: u32,
}

/// Load diagnostic summaries for **all** cached files.
///
/// Returns `(uri_string, errors, warnings)` for every entry in the
/// `file_cache` table that has at least one error or warning.
pub fn load_all_diag_summaries() -> Vec<DiagSummary> {
    let Some(db) = cache_db::db() else { return vec![] };
    let Ok(read_txn) = db.begin_read() else { return vec![] };
    let Ok(table) = read_txn.open_table(cache_db::FILE_CACHE_TABLE) else { return vec![] };
    let Ok(iter) = table.iter() else { return vec![] };

    let mut result = Vec::new();
    for entry_result in iter {
        let (key_guard, val_guard) = match entry_result {
            Ok(kv) => kv,
            Err(_) => continue,
        };
        let uri_str: &str = key_guard.value();
        let bytes: &[u8] = val_guard.value();

        let entry: CacheEntry = match bitcode::deserialize(bytes) {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.diag_errors > 0 || entry.diag_warnings > 0 {
            result.push(DiagSummary {
                uri: uri_str.to_string(),
                errors: entry.diag_errors,
                warnings: entry.diag_warnings,
            });
        }
    }
    result
}


/// Delete **all** cache entries.
#[allow(dead_code)]
pub fn purge_all() {
    let Some(db) = cache_db::db() else { return };
    let write_txn = match db.begin_write() {
        Ok(t) => t,
        Err(e) => {
            error!("file_cache: begin_write (purge): {}", e);
            return;
        }
    };
    let _ = write_txn.delete_table(cache_db::FILE_CACHE_TABLE);
    if let Err(e) = write_txn.commit() {
        error!("file_cache: commit purge: {}", e);
    }
    info!("file_cache: purge_all completed");
}

/// Delete cache entries for the given set of URIs.
pub fn purge_set(uris: &HashSet<Url>) {
    if uris.is_empty() {
        return;
    }
    let Some(db) = cache_db::db() else { return };
    let write_txn = match db.begin_write() {
        Ok(t) => t,
        Err(e) => {
            error!("file_cache: begin_write (purge_set): {}", e);
            return;
        }
    };
    {
        let mut table = match write_txn.open_table(cache_db::FILE_CACHE_TABLE) {
            Ok(t) => t,
            Err(_) => return,
        };
        for uri in uris {
            let _ = table.remove(uri.as_str());
        }
    }
    if let Err(e) = write_txn.commit() {
        error!("file_cache: purge_set commit: {}", e);
    } else {
        info!("file_cache: purge_set removed {} entries", uris.len());
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

// ─── Internals ───────────────────────────────────────────────────────────────

fn remove_entry(uri: &Url) {
    let Some(db) = cache_db::db() else { return };
    let write_txn = match db.begin_write() {
        Ok(t) => t,
        Err(_) => return,
    };
    {
        if let Ok(mut table) = write_txn.open_table(cache_db::FILE_CACHE_TABLE) {
            let _ = table.remove(uri.as_str());
        }
    }
    let _ = write_txn.commit();
}

