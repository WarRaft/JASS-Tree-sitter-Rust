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
//! Cache file: `$CACHE_DIR/jass-tree-sitter-scope.bin`

use log::{error, info};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
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
}

// ─── Inner storage ───────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
struct ScopeInner {
    /// name → list of entries.
    by_name: HashMap<String, Vec<GlobalEntry>>,
    /// uri → set of names declared in that file.
    by_uri: HashMap<Url, HashSet<String>>,
    /// uri → SHA-256 of file content at the time the entries were built.
    hashes: HashMap<Url, [u8; 32]>,
}

// ─── Cache ───────────────────────────────────────────────────────────────────

const CACHE_FILE: &str = "jass-tree-sitter-scope.bin";

fn cache_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join(CACHE_FILE))
}

// ─── ScopeResolver ──────────────────────────────────────────────────────────

/// Thread-safe, persistent global-scope symbol index.
pub struct ScopeResolver {
    inner: RwLock<ScopeInner>,
}

impl ScopeResolver {
    // ─── Construction ────────────────────────────────────────────────────

    /// Create an empty resolver (for tests).
    #[cfg(test)]
    pub(crate) fn new_empty() -> Self {
        Self {
            inner: RwLock::new(ScopeInner::default()),
        }
    }

    /// Load from disk cache, or create empty.
    fn load() -> Self {
        let inner = if let Some(path) = cache_path() {
            if path.exists() {
                match fs::read(&path) {
                    Ok(data) => match bincode::deserialize::<ScopeInner>(&data) {
                        Ok(si) => {
                            info!(
                                "scope_resolver: loaded {} names, {} files from {}",
                                si.by_name.len(),
                                si.by_uri.len(),
                                path.display()
                            );
                            si
                        }
                        Err(e) => {
                            error!("scope_resolver: deserialize: {}", e);
                            let _ = fs::remove_file(&path);
                            ScopeInner::default()
                        }
                    },
                    Err(e) => {
                        error!("scope_resolver: read: {}", e);
                        ScopeInner::default()
                    }
                }
            } else {
                ScopeInner::default()
            }
        } else {
            ScopeInner::default()
        };

        Self {
            inner: RwLock::new(inner),
        }
    }

    /// Persist to disk.
    fn save(inner: &ScopeInner) {
        let Some(path) = cache_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match bincode::serialize(inner) {
            Ok(data) => {
                if let Err(e) = fs::write(&path, &data) {
                    error!("scope_resolver: write: {}", e);
                }
            }
            Err(e) => error!("scope_resolver: serialize: {}", e),
        }
    }

    // ─── Mutation ────────────────────────────────────────────────────────

    /// Update the index for `uri` — removes old entries, adds new ones.
    ///
    /// `content_hash` is the SHA-256 of the file content at parse time;
    /// used by [`is_stale`] to detect changes without re-reading the file.
    ///
    /// Automatically persists to disk.
    pub fn update_file(
        &self,
        uri: &Url,
        content_hash: [u8; 32],
        entries: Vec<GlobalEntry>,
    ) {
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
        for entry in entries {
            names.insert(entry.name.clone());
            inner
                .by_name
                .entry(entry.name.clone())
                .or_default()
                .push(entry);
        }
        inner.by_uri.insert(uri.clone(), names);
        inner.hashes.insert(uri.clone(), content_hash);

        Self::save(&inner);
    }

    /// Remove all entries for `uri`.
    #[allow(dead_code)]
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
        Self::save(&inner);
    }

    // ─── Queries (read lock) ────────────────────────────────────────────

    /// Look up all entries for `name` in namespace `ns`, filtered to
    /// declarations from `visible_uris`.
    ///
    /// Returns an empty vec if no matches.
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn content_hash(&self, uri: &Url) -> Option<[u8; 32]> {
        let inner = self.inner.read().unwrap();
        inner.hashes.get(uri).copied()
    }

    /// All URIs known to the resolver.
    #[allow(dead_code)]
    pub fn all_uris(&self) -> Vec<Url> {
        let inner = self.inner.read().unwrap();
        inner.by_uri.keys().cloned().collect()
    }

    /// Total number of indexed symbols.
    #[allow(dead_code)]
    pub fn symbol_count(&self) -> usize {
        let inner = self.inner.read().unwrap();
        inner.by_name.values().map(|v| v.len()).sum()
    }

    /// Number of indexed files.
    #[allow(dead_code)]
    pub fn file_count(&self) -> usize {
        let inner = self.inner.read().unwrap();
        inner.by_uri.len()
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
        Self::save(&inner);
    }
}

