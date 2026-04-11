//! Persistent global-scope symbol index with O(1) name lookup.
//!
//! Every file's **global-scope** declarations (functions, natives, types,
//! global variables) are indexed here by name.  The resolver is the single
//! source of truth for cross-file symbol resolution and is persisted to disk
//! so it survives server restarts.
//!
//! # Design
//!
//! ```text
//! by_name:  HashMap<String, Vec<GlobalEntry>>   ← O(1) by name
//! by_uri:   HashMap<Url, HashSet<String>>       ← O(1) file removal
//! hashes:   HashMap<Url, [u8; 32]>              ← staleness check
//! ```
//!
//! Thread-safety is provided by `RwLock<ScopeInner>`.
//!
//! Persistence: per-file entries stored in the `scope` table of the shared
//! `redb` database.  Only the changed file is written on each update (not the
//! entire index).

use crate::util::cache_db;
use log::{error, info};
use once_cell::sync::Lazy;
use redb::{ReadableDatabase, ReadableTable};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;
use url::Url;

// ─── Global instance ─────────────────────────────────────────────────────────

/// Global scope resolver.  Loaded from disk on first access.
pub static SCOPE_RESOLVER: Lazy<ScopeResolver> = Lazy::new(ScopeResolver::load);

// ─── Types ───────────────────────────────────────────────────────────────────

/// Symbol namespace — JASS separates functions and variables/types by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolNS {
    /// `function` or `native` — resolves in the function namespace.
    Func,
    /// Global variable, constant, or `type` — resolves in the variable namespace.
    Var,
}

/// A single global-scope declaration in one file.
///
/// Stored in the name index; contains enough information for cross-file
/// resolution, hover tooltips, and completion items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalEntry {
    /// URI of the file containing the declaration.
    pub uri: Url,
    /// Symbol name.
    pub name: String,
    /// Namespace — JASS always uses `""` (global scope).
    /// AngelScript will use the enclosing `namespace Foo { … }` name.
    pub namespace: String,
    /// Namespace.
    pub ns: SymbolNS,
    /// `start_byte` of the declaring node in the origin file.
    /// Used as `DeclKey` for cross-file reference linking.
    pub decl_key: usize,

    // ── Metadata for hover / completion ──────────────────────────────────

    /// Type name for variables; `None` for functions/natives.
    pub type_name: Option<String>,
    /// Parameter list `(name, type)` for functions/natives; empty for vars.
    pub params: Vec<(String, String)>,
    /// Return type for functions/natives; `None` for variables.
    pub return_type: Option<String>,
    /// `true` for `constant` global variables.
    pub is_constant: bool,
    /// `true` for `array` global variables.
    pub is_array: bool,
    /// `//*` doc comment (markdown) attached to this declaration.
    pub doc_comment: Option<String>,
}

// ─── Per-file data stored in redb ────────────────────────────────────────────

/// What is persisted per file in the `scope` table.
#[derive(Serialize, Deserialize)]
struct ScopeFileData {
    hash: [u8; 32],
    entries: Vec<GlobalEntry>,
    /// Version key at the time the entry was written.
    /// Old entries without this field deserialize to `""` and are skipped.
    #[serde(default)]
    version: String,
}

// ─── Inner storage ───────────────────────────────────────────────────────────

#[derive(Default)]
struct ScopeInner {
    /// name → list of entries.
    by_name: HashMap<String, Vec<GlobalEntry>>,
    /// uri → set of names declared in that file.
    by_uri: HashMap<Url, HashSet<String>>,
    /// uri → SHA-256 of file content at the time the entries were built.
    hashes: HashMap<Url, [u8; 32]>,
}

// ─── ScopeResolver ──────────────────────────────────────────────────────────

/// Thread-safe, persistent global-scope symbol index.
pub struct ScopeResolver {
    inner: RwLock<ScopeInner>,
}

#[allow(dead_code)]
impl ScopeResolver {
    // ─── Construction ────────────────────────────────────────────────────

    /// Create an empty resolver (for tests).
    #[cfg(test)]
    pub(crate) fn new_empty() -> Self {
        Self {
            inner: RwLock::new(ScopeInner::default()),
        }
    }

    /// Load from the redb database, or create empty.
    fn load() -> Self {
        let mut si = ScopeInner::default();

        if let Some(db) = cache_db::db() {
            if let Ok(read_txn) = db.begin_read() {
                if let Ok(table) = read_txn.open_table(cache_db::SCOPE_TABLE) {
                    if let Ok(iter) = table.iter() {
                        let mut loaded = 0usize;
                        for entry_result in iter {
                            let (key_guard, val_guard): (redb::AccessGuard<&str>, redb::AccessGuard<&[u8]>) = match entry_result {
                                Ok(kv) => kv,
                                Err(_) => continue,
                            };
                            let uri_str: &str = key_guard.value();
                            let bytes: &[u8] = val_guard.value();

                            let uri = match Url::parse(uri_str) {
                                Ok(u) => u,
                                Err(_) => continue,
                            };

                            let file_data: ScopeFileData =
                                match bitcode::deserialize(bytes) {
                                    Ok(d) => d,
                                    Err(_) => continue,
                                };

                            // Skip entries written by a different version.
                            if file_data.version != cache_db::version_key() {
                                continue;
                            }

                            let mut names = HashSet::new();
                            for entry in file_data.entries {
                                names.insert(entry.name.clone());
                                si.by_name
                                    .entry(entry.name.clone())
                                    .or_default()
                                    .push(entry);
                            }
                            si.by_uri.insert(uri.clone(), names);
                            si.hashes.insert(uri, file_data.hash);
                            loaded += 1;
                        }
                        info!(
                            "scope_resolver: loaded {} names, {} files from redb",
                            si.by_name.len(),
                            loaded
                        );
                    }
                }
            }
        }

        Self {
            inner: RwLock::new(si),
        }
    }

    /// Persist one file's entries to redb.
    fn save_file(uri: &Url, hash: [u8; 32], entries: &[GlobalEntry]) {
        let Some(db) = cache_db::db() else { return };

        let file_data = ScopeFileData {
            hash,
            entries: entries.to_vec(),
            version: cache_db::version_key(),
        };
        let data = match bitcode::serialize(&file_data) {
            Ok(d) => d,
            Err(e) => {
                error!("scope_resolver: serialize {}: {}", uri, e);
                return;
            }
        };

        let write_txn = match db.begin_write() {
            Ok(t) => t,
            Err(e) => {
                error!("scope_resolver: begin_write: {}", e);
                return;
            }
        };
        {
            let mut table = match write_txn.open_table(cache_db::SCOPE_TABLE) {
                Ok(t) => t,
                Err(e) => {
                    error!("scope_resolver: open table: {}", e);
                    return;
                }
            };
            if let Err(e) = table.insert(uri.as_str(), data.as_slice()) {
                error!("scope_resolver: insert {}: {}", uri, e);
                return;
            }
        }
        if let Err(e) = write_txn.commit() {
            error!("scope_resolver: commit: {}", e);
        }
    }

    /// Remove one file's entries from redb.
    fn delete_file_from_db(uri: &Url) {
        let Some(db) = cache_db::db() else { return };
        let write_txn = match db.begin_write() {
            Ok(t) => t,
            Err(_) => return,
        };
        {
            if let Ok(mut table) = write_txn.open_table(cache_db::SCOPE_TABLE) {
                let _ = table.remove(uri.as_str());
            }
        }
        let _ = write_txn.commit();
    }

    // ─── Mutation ────────────────────────────────────────────────────────

    /// Update the index for `uri` — removes old entries, adds new ones.
    ///
    /// `content_hash` is the SHA-256 of the file content at parse time;
    /// used by [`is_stale`] to detect changes without re-reading the file.
    ///
    /// **Fast path:** if the stored hash already matches `content_hash`,
    /// the entries are assumed unchanged and the function returns immediately
    /// — no write lock, no serialization, no disk I/O.
    ///
    /// Automatically persists the changed file to redb.
    pub fn update_file(
        &self,
        uri: &Url,
        content_hash: [u8; 32],
        entries: Vec<GlobalEntry>,
    ) {
        // Fast path: if the hash is unchanged, the entries haven't changed
        // either (they're derived deterministically from the same content).
        {
            let inner = self.inner.read().unwrap();
            if inner.hashes.get(uri) == Some(&content_hash) {
                return;
            }
        }

        let mut inner = self.inner.write().unwrap();

        // 1. Remove old entries for this URI.
        if let Some(old_names) = inner.by_uri.remove(uri) {
            for name in &old_names {
                if let Some(vec) = inner.by_name.get_mut(name) {
                    vec.retain(|e| &e.uri != uri);
                    if vec.is_empty() {
                        inner.by_name.remove(name);
                    }
                }
            }
        }

        // 2. Insert new entries.
        let mut names = HashSet::new();
        for entry in &entries {
            names.insert(entry.name.clone());
            inner
                .by_name
                .entry(entry.name.clone())
                .or_default()
                .push(entry.clone());
        }
        inner.by_uri.insert(uri.clone(), names);
        inner.hashes.insert(uri.clone(), content_hash);

        // 3. Persist only this file to redb (not the entire index).
        Self::save_file(uri, content_hash, &entries);
    }

    /// Remove all entries for `uri`.
    pub fn remove_file(&self, uri: &Url) {
        let mut inner = self.inner.write().unwrap();
        if let Some(old_names) = inner.by_uri.remove(uri) {
            for name in &old_names {
                if let Some(vec) = inner.by_name.get_mut(name) {
                    vec.retain(|e| &e.uri != uri);
                    if vec.is_empty() {
                        inner.by_name.remove(name);
                    }
                }
            }
        }
        inner.hashes.remove(uri);
        Self::delete_file_from_db(uri);
    }

    /// Remove entries for a set of URIs (batch).
    pub fn remove_files(&self, uris: &HashSet<Url>) {
        if uris.is_empty() {
            return;
        }
        let mut inner = self.inner.write().unwrap();
        for uri in uris {
            if let Some(old_names) = inner.by_uri.remove(uri) {
                for name in &old_names {
                    if let Some(vec) = inner.by_name.get_mut(name) {
                        vec.retain(|e| &e.uri != uri);
                        if vec.is_empty() {
                            inner.by_name.remove(name);
                        }
                    }
                }
            }
            inner.hashes.remove(uri);
        }

        // Batch-remove from redb.
        if let Some(db) = cache_db::db() {
            if let Ok(write_txn) = db.begin_write() {
                if let Ok(mut table) = write_txn.open_table(cache_db::SCOPE_TABLE) {
                    for uri in uris {
                        let _ = table.remove(uri.as_str());
                    }
                }
                let _ = write_txn.commit();
            }
        }

        info!("scope_resolver: remove_files removed {} files", uris.len());
    }

    // ─── Queries (read lock) ────────────────────────────────────────────

    /// Look up all entries for `name` in namespace `ns`, filtered to
    /// declarations from `visible_uris`.
    ///
    /// Returns an empty vec if no matches.
    pub fn resolve(
        &self,
        name: &str,
        ns: SymbolNS,
        visible: &HashSet<Url>,
    ) -> Vec<GlobalEntry> {
        let inner = self.inner.read().unwrap();
        inner
            .by_name
            .get(name)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| e.ns == ns && visible.contains(&e.uri))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Return **all** entries whose `uri` is in `visible_uris`.
    ///
    /// Used by `parse.rs` to build the full `imported_symbols` list for
    /// the cursor in a single pass over the index.
    pub fn all_visible(&self, visible_uris: &HashSet<Url>) -> Vec<GlobalEntry> {
        let inner = self.inner.read().unwrap();
        let mut result = Vec::new();
        for entries in inner.by_name.values() {
            for entry in entries {
                if visible_uris.contains(&entry.uri) {
                    result.push(entry.clone());
                }
            }
        }
        result
    }

    /// Check whether the cached data for `uri` is stale (content changed).
    ///
    /// Returns `true` if:
    /// - `uri` is not in the index, or
    /// - the stored hash differs from `current_hash`.
    pub fn is_stale(&self, uri: &Url, current_hash: &[u8; 32]) -> bool {
        let inner = self.inner.read().unwrap();
        match inner.hashes.get(uri) {
            Some(h) => h != current_hash,
            None => true,
        }
    }

    /// Cheap fingerprint of a file's **exported** symbol set.
    ///
    /// The fingerprint changes when names are added, removed, or when the
    /// namespace of a name changes.  Body-only edits do NOT change the
    /// fingerprint, so the caller can skip cascade re-parses.
    ///
    /// Returns `None` if `uri` is unknown.
    pub fn export_fingerprint(&self, uri: &Url) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        let inner = self.inner.read().unwrap();
        let names = inner.by_uri.get(uri)?;

        let mut sorted: Vec<&String> = names.iter().collect();
        sorted.sort();

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for name in &sorted {
            name.hash(&mut hasher);
            // Also hash the namespace(s) for each name so that changing a
            // variable to a function (same name) is detected.
            if let Some(entries) = inner.by_name.get(name.as_str()) {
                for e in entries {
                    if &e.uri == uri {
                        (e.ns as u8).hash(&mut hasher);
                    }
                }
            }
        }
        Some(hasher.finish())
    }

    /// Get the content hash for `uri`, if known.
    pub fn content_hash(&self, uri: &Url) -> Option<[u8; 32]> {
        let inner = self.inner.read().unwrap();
        inner.hashes.get(uri).copied()
    }

    /// All URIs known to the resolver.
    pub fn all_uris(&self) -> Vec<Url> {
        let inner = self.inner.read().unwrap();
        inner.by_uri.keys().cloned().collect()
    }

    /// Return all entries for a single `uri`.
    pub fn entries_for_uri(&self, uri: &Url) -> Vec<GlobalEntry> {
        let inner = self.inner.read().unwrap();
        let names = match inner.by_uri.get(uri) {
            Some(n) => n,
            None => return Vec::new(),
        };
        let mut result = Vec::new();
        for name in names {
            if let Some(entries) = inner.by_name.get(name) {
                for e in entries {
                    if &e.uri == uri {
                        result.push(e.clone());
                    }
                }
            }
        }
        result
    }

    /// Total number of indexed symbols.
    pub fn symbol_count(&self) -> usize {
        let inner = self.inner.read().unwrap();
        inner.by_name.values().map(|v| v.len()).sum()
    }

    /// Number of indexed files.
    pub fn file_count(&self) -> usize {
        let inner = self.inner.read().unwrap();
        inner.by_uri.len()
    }

    /// Remove **all** entries and persisted cache — used by the forced rescan.
    pub fn clear_all(&self) {
        let mut inner = self.inner.write().unwrap();
        let count = inner.by_uri.len();
        inner.by_name.clear();
        inner.by_uri.clear();
        inner.hashes.clear();

        // Purge the entire table from redb.
        if let Some(db) = cache_db::db() {
            if let Ok(write_txn) = db.begin_write() {
                let _ = write_txn.delete_table(cache_db::SCOPE_TABLE);
                let _ = write_txn.commit();
            }
        }

        info!("scope_resolver: clear_all removed {} files", count);
    }

    /// Remove entries for all URIs **not** in `keep`.
    pub fn gc(&self, keep: &HashSet<Url>) {
        let mut inner = self.inner.write().unwrap();
        let to_remove: Vec<Url> = inner
            .by_uri
            .keys()
            .filter(|u| !keep.contains(u))
            .cloned()
            .collect();

        if to_remove.is_empty() {
            return;
        }

        // Batch-remove from redb.
        if let Some(db) = cache_db::db() {
            if let Ok(write_txn) = db.begin_write() {
                if let Ok(mut table) = write_txn.open_table(cache_db::SCOPE_TABLE) {
                    for uri in &to_remove {
                        let _ = table.remove(uri.as_str());
                    }
                }
                let _ = write_txn.commit();
            }
        }

        for uri in &to_remove {
            if let Some(old_names) = inner.by_uri.remove(uri) {
                for name in &old_names {
                    if let Some(vec) = inner.by_name.get_mut(name) {
                        vec.retain(|e| &e.uri != uri);
                        if vec.is_empty() {
                            inner.by_name.remove(name);
                        }
                    }
                }
            }
            inner.hashes.remove(uri);
        }

        info!("scope_resolver: gc removed {} stale files", to_remove.len());
    }

    /// Return **all** entries across every indexed file.
    pub fn all_entries(&self) -> Vec<GlobalEntry> {
        let inner = self.inner.read().unwrap();
        inner
            .by_name
            .values()
            .flat_map(|v| v.iter().cloned())
            .collect()
    }
}

