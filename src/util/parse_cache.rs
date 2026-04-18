//! Central per-file storage — replaces scattered per-feature DashMaps.
//!
//! **`PARSE_CACHE`** holds an `Arc<ParseSnapshot>` per URI.  A snapshot is
//! the immutable, atomic output of one successful parse.  All LSP request
//! handlers read from the snapshot — no separate folding / symbol /
//! semantic / diagnostic / link / ref maps.
//!
//! **`CANCEL_TOKENS`** holds a `CancellationToken` per URI.  When a new
//! edit arrives the old token is cancelled, causing the in-flight parse
//! task to bail out before it can store stale results.
//!
//! **`LSP_WRITER`** is set once from `main()` so that background tasks
//! (cascade re-parses) can push notifications without threading the
//! writer through every call.

use std::collections::HashSet;
use std::sync::Arc;

use dashmap::{DashMap, DashSet};
use log::info;
use once_cell::sync::Lazy;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::lng::symbol::FileSymbols;
use crate::lng::jass::type_map::TypeMap;
use crate::http::color::ColorInformation;
use crate::http::diagnostic::Diagnostic;
use crate::http::document_link::DocumentLink;
use crate::http::document_symbol::DocumentSymbol;
use crate::http::folding::FoldingRange;
use crate::http::inlay_hint::{InlayHint, InlayHintKind};
use crate::http::ref_map::{DeclKey, RefMap};
use crate::http::semantic::hub::Hub;
use std::sync::RwLock;

// ─── ParseSnapshot ───────────────────────────────────────────────────────────

/// Immutable snapshot of **all** computed LSP data for a single file.
///
/// Produced atomically by one successful parse.  Wrapped in `Arc` so that
/// readers and the next parse task can coexist without locking.
///
/// The `semantic` field uses `RwLock` for interior mutability: formatting
/// adjusts token column positions in-place without rebuilding the entire
/// snapshot.
pub struct ParseSnapshot {
    pub folding: Vec<FoldingRange>,
    pub symbols: Vec<DocumentSymbol>,
    pub semantic: RwLock<Hub>,
    pub diagnostics: Vec<Diagnostic>,
    pub links: Vec<DocumentLink>,
    pub ref_map: RefMap,
    pub file_symbols: FileSymbols,
    /// Per-declaration resolved types — foundation for type checking,
    /// compile-time evaluation, inlay hints, and build.
    pub _type_map: TypeMap,
    /// Inlay hints for type annotations (shown when `//set hint type`).
    pub type_hints: Vec<InlayHint>,
    /// Inlay hints from `//import-ujapi!` — always visible (version tag).
    pub ujapi_hints: Vec<InlayHint>,
    /// DeclKeys that belong to function / native declarations.
    /// DeclKeys that belong to function / native declarations.
    pub func_decl_keys: HashSet<DeclKey>,
    /// DeclKeys that belong to variable declarations (globals + locals).
    pub var_decl_keys: HashSet<DeclKey>,
    /// DeclKeys that belong to function parameter declarations.
    pub arg_decl_keys: HashSet<DeclKey>,
    /// Color information for `|cAARRGGBB` in strings and `0xAARRGGBB` hex literals.
    pub colors: Vec<ColorInformation>,
}

impl ParseSnapshot {
    /// Compute all inlay hints for this snapshot.
    ///
    /// Merges ujapi version hints (always visible), reference-ID hints
    /// (`ref` tag), and type-annotation hints (`type` tag) based on the
    /// `//set hint ref type` directive.
    pub fn all_inlay_hints(&self) -> Vec<InlayHint> {
        let mut hints = Vec::new();

        // ujapi hints: always visible (version tag after import path).
        hints.extend(self.ujapi_hints.iter().cloned());

        let settings = &self.file_symbols.file_settings;
        let hint_value = settings.get("hint").map(|v| v.as_str()).unwrap_or("");
        let ref_tip = hint_value.split_whitespace().any(|w| w == "ref");
        let type_tip = hint_value.split_whitespace().any(|w| w == "type");

        if !ref_tip && !type_tip {
            return hints;
        }

        // ref: debug reference-ID hints.
        if ref_tip {
            let ref_map = &self.ref_map;
            for span in &ref_map.spans {
                let label = if span.is_external {
                    ref_map
                        .external_decls
                        .get(&span.decl_key)
                        .map(|ext| {
                            let parts: Vec<String> = ext.origins.iter().map(|o| {
                                let path = o.uri.path();
                                let fname = path.rsplit('/').next().unwrap_or(path);
                                match o.origin_decl_key {
                                    Some(ok) => format!("{}#{}", fname, ok),
                                    None => fname.to_string(),
                                }
                            }).collect();
                            format!("\u{2192}{}", parts.join(","))
                        })
                        .unwrap_or_else(|| format!("#{}", span.decl_key))
                } else {
                    format!("#{}", span.decl_key)
                };

                hints.push(InlayHint {
                    position: span.range.end.clone(),
                    label,
                    kind: InlayHintKind::Type,
                    byte_offset: 0,
                });
            }
        }

        // type: type-annotation hints.
        if type_tip {
            hints.extend(self.type_hints.iter().cloned());
        }

        hints
    }

    /// Like [`all_inlay_hints`] but uses a request-level override string
    /// (comma or space-separated tags: `ref`, `type`) instead of file settings.
    #[allow(dead_code)]
    pub fn all_inlay_hints_filtered(&self, requested: &str) -> Vec<InlayHint> {
        let mut hints = Vec::new();
        hints.extend(self.ujapi_hints.iter().cloned());

        let has = |tag: &str| requested.split(|c: char| c == ',' || c.is_whitespace())
            .any(|w| w == tag);
        let ref_tip = has("ref");
        let type_tip = has("type");

        if !ref_tip && !type_tip {
            return hints;
        }

        if ref_tip {
            let ref_map = &self.ref_map;
            for span in &ref_map.spans {
                let label = if span.is_external {
                    ref_map
                        .external_decls
                        .get(&span.decl_key)
                        .map(|ext| {
                            let parts: Vec<String> = ext.origins.iter().map(|o| {
                                let path = o.uri.path();
                                let fname = path.rsplit('/').next().unwrap_or(path);
                                match o.origin_decl_key {
                                    Some(ok) => format!("{}#{}", fname, ok),
                                    None => fname.to_string(),
                                }
                            }).collect();
                            format!("\u{2192}{}", parts.join(","))
                        })
                        .unwrap_or_else(|| format!("#{}", span.decl_key))
                } else {
                    format!("#{}", span.decl_key)
                };
                hints.push(InlayHint {
                    position: span.range.end.clone(),
                    label,
                    kind: InlayHintKind::Type,
                    byte_offset: 0,
                });
            }
        }

        if type_tip {
            hints.extend(self.type_hints.iter().cloned());
        }

        hints
    }
}

// ─── Global stores ───────────────────────────────────────────────────────────

/// Per-URI last-good parse snapshot.
pub static PARSE_CACHE: Lazy<DashMap<Url, Arc<ParseSnapshot>>> = Lazy::new(DashMap::new);

/// Read-only accessor: returns the snapshot from `PARSE_CACHE` (open files)
/// or reconstructs a minimal one from the persistent **disk cache**
/// (`file_cache`), but **never inserts** into `PARSE_CACHE`.
///
/// This keeps `PARSE_CACHE` strictly for files the user has open in the
/// editor.  Cross-file helpers (`all_visible_entries`, `resolve_entries`,
/// etc.) use this so that closed files do not "leak" into memory.
pub fn peek_or_load(uri: &Url) -> Option<Arc<ParseSnapshot>> {
    // 1. Fast path — already in memory (file is open).
    if let Some(entry) = PARSE_CACHE.get(uri) {
        return Some(Arc::clone(entry.value()));
    }
    // 2. Disk cache — reconstruct a partial snapshot WITHOUT caching it.
    let cached = crate::util::file_cache::load_if_fresh(uri)?;
    Some(Arc::new(ParseSnapshot {
        folding: Vec::new(),
        symbols: Vec::new(),
        semantic: std::sync::RwLock::new(Default::default()),
        diagnostics: Vec::new(),
        links: Vec::new(),
        ref_map: cached.ref_map,
        file_symbols: cached.symbols,
        _type_map: Default::default(),
        type_hints: Vec::new(),
        ujapi_hints: Vec::new(),
        func_decl_keys: cached.func_decl_keys,
        var_decl_keys: cached.var_decl_keys,
        arg_decl_keys: cached.arg_decl_keys,
        colors: Vec::new(),
    }))
}


/// Per-URI cancellation token.
pub static CANCEL_TOKENS: Lazy<DashMap<Url, CancellationToken>> = Lazy::new(DashMap::new);


/// Pending-import waiters: dependency URI → set of files waiting for it.
///
/// When [`ensure_file_symbols`](crate::lng::jass::parse) cannot load symbols
/// for a dependency (file not on disk yet, still parsing, cache miss, etc.),
/// the requesting file is registered here.  Once the dependency finishes
/// parsing, [`drain_pending`] returns (and removes) the waiters so they can
/// be cascade-re-parsed.
pub static PENDING_IMPORTS: Lazy<DashMap<Url, HashSet<Url>>> = Lazy::new(DashMap::new);

/// Pending-import waiters: dependency URI → set of files waiting for it.
pub fn register_pending(dep: &Url, waiter: &Url) {
    PENDING_IMPORTS
        .entry(dep.clone())
        .or_default()
        .insert(waiter.clone());
}

/// Drain and return all files that were waiting for `dep`.
pub fn drain_pending(dep: &Url) -> Vec<Url> {
    PENDING_IMPORTS
        .remove(dep)
        .map(|(_, set)| set.into_iter().collect())
        .unwrap_or_default()
}

/// Guard against cyclic cascade re-parses.
///
/// While a URI is in this set, no other task may cascade-re-parse it.
/// Prevents the A→cascade(B)→cascade(A)→… infinite loop.
pub static REPARSE_GUARD: Lazy<DashSet<Url>> = Lazy::new(DashSet::new);

/// Maximum number of peer files re-parsed in a single cascade round.
///
/// Prevents unbounded work in pathological import topologies (deep chains,
/// many-to-many).
pub const MAX_CASCADE_PEERS: usize = 128;

/// RAII guard that inserts `uri` into [`REPARSE_GUARD`] on creation and
/// removes it on drop — even on early `?` returns or panics.
pub struct CascadeGuard {
    uri: Url,
}

impl CascadeGuard {
    /// Try to enter the cascade guard.
    ///
    /// Returns `Some(guard)` if `uri` was **not** already in the set.
    /// Returns `None` if another cascade is already in progress for `uri`.
    pub fn try_enter(uri: &Url) -> Option<Self> {
        if REPARSE_GUARD.insert(uri.clone()) {
            Some(Self { uri: uri.clone() })
        } else {
            None
        }
    }
}

impl Drop for CascadeGuard {
    fn drop(&mut self) {
        REPARSE_GUARD.remove(&self.uri);
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Cancel any in-flight parse for `uri` and return a fresh token.
pub fn new_cancel_token(uri: &Url) -> CancellationToken {
    let had_old = CANCEL_TOKENS.contains_key(uri);
    if let Some(old) = CANCEL_TOKENS.get(uri) {
        old.cancel();
    }
    let token = CancellationToken::new();
    CANCEL_TOKENS.insert(uri.clone(), token.clone());
    crate::debug_log!(
        "parse_cache::new_cancel_token uri={}, replaced_existing={}",
        uri.path(),
        had_old
    );
    token
}


// ─── DidClose cleanup ────────────────────────────────────────────────────────

/// Evict per-editor state for a closed file and, if the whole import tree
/// has no more open files, evict the tree's `PARSE_CACHE` / scope entries
/// so that memory is reclaimed.
///
/// Returns the set of URIs whose diagnostics were cleared (caller should
/// send empty `publishDiagnostics` for each).
pub fn evict_closed_file(uri: &Url) -> Vec<Url> {
    use crate::util::import_graph::IMPORT_GRAPH;
    use crate::util::roper::uri_map::ROPE_MAP;
    use crate::util::tree_map::{PARSER_MAP, TREE_MAP};
    use crate::util::uri_map::LNG_URI_MAP;

    // 1. Remove per-editor state for the closed file.
    ROPE_MAP.remove(uri);
    TREE_MAP.remove(uri);
    PARSER_MAP.remove(uri);
    LNG_URI_MAP.remove(uri);
    CANCEL_TOKENS.remove(uri);
    PARSE_DESIRED.remove(uri);
    PARSE_DONE_TX.remove(uri);

    // 2. Determine the import-tree the closed file belongs to.
    let tree_uris = IMPORT_GRAPH.tree_for_uri(uri);

    // 3. Check if any file in that tree is still open in the editor.
    let any_open = tree_uris.iter().any(|u| ROPE_MAP.contains_key(u));

    if any_open {
        // At least one file is still open — keep PARSE_CACHE intact.
        return vec![];
    }

    // 4. No open files remain in this tree — evict everything.
    let mut evicted: Vec<Url> = Vec::with_capacity(tree_uris.len());
    for tree_uri in &tree_uris {
        if PARSE_CACHE.remove(tree_uri).is_some() {
            evicted.push(tree_uri.clone());
        }
        CANCEL_TOKENS.remove(tree_uri);
        PARSE_DESIRED.remove(tree_uri);
        PARSE_DONE_TX.remove(tree_uri);
    }


    info!(
        "evict_closed_file: tree for {} — evicted {} file(s)",
        uri.path().rsplit('/').next().unwrap_or(""),
        evicted.len(),
    );

    evicted
}

/// Check if `target_uri` is considered **frozen** (imported via `//import!`
/// by anyone in the graph).  If *any* file imports it with `//import!`, the
/// target is frozen — even if another file imports it with plain `//import`.
///
/// Checks the import graph's eagerly-updated set first, then falls back to
/// PARSE_CACHE snapshots.
pub fn is_uri_frozen(target_uri: &Url) -> bool {
    if crate::util::import_graph::IMPORT_GRAPH.is_frozen(target_uri) {
        return true;
    }
    PARSE_CACHE.iter().any(|entry| {
        entry.value().file_symbols.frozen_imports.contains(target_uri)
    })
}

/// Check if `uri` is marked as an **entry point** (contains `//entry` directive).
pub fn is_uri_entry(uri: &Url) -> bool {
    PARSE_CACHE
        .get(uri)
        .map(|snap| snap.file_symbols.is_entry)
        .unwrap_or(false)
}

/// Collect all URIs that are marked as entry points (`//entry` directive).
pub fn entry_uris() -> Vec<Url> {
    PARSE_CACHE
        .iter()
        .filter(|entry| entry.value().file_symbols.is_entry)
        .map(|entry| entry.key().clone())
        .collect()
}


/// Export diff: compare the old and new exported symbol names.
///
/// Returns `true` if the exports changed (names added / removed / ns changed).
pub fn exports_changed(old: Option<&ParseSnapshot>, new: &ParseSnapshot) -> bool {
    let old = match old {
        Some(o) => o,
        None => return true, // first parse → always changed
    };

    // Compare function names
    let old_funcs: HashSet<&str> = old.file_symbols.functions.iter().map(|f| f.name.as_str()).collect();
    let new_funcs: HashSet<&str> = new.file_symbols.functions.iter().map(|f| f.name.as_str()).collect();
    if old_funcs != new_funcs {
        return true;
    }

    // Compare native names
    let old_natives: HashSet<&str> = old.file_symbols.natives.iter().map(|n| n.name.as_str()).collect();
    let new_natives: HashSet<&str> = new.file_symbols.natives.iter().map(|n| n.name.as_str()).collect();
    if old_natives != new_natives {
        return true;
    }

    // Compare global variable names
    let old_globals: HashSet<&str> = old.file_symbols.globals.iter().map(|g| g.name.as_str()).collect();
    let new_globals: HashSet<&str> = new.file_symbols.globals.iter().map(|g| g.name.as_str()).collect();
    if old_globals != new_globals {
        return true;
    }

    // Compare type names
    let old_types: HashSet<&str> = old.file_symbols.types.iter().map(|t| t.name.as_str()).collect();
    let new_types: HashSet<&str> = new.file_symbols.types.iter().map(|t| t.name.as_str()).collect();
    if old_types != new_types {
        return true;
    }

    // Compare AS-specific symbol names
    let old_classes: HashSet<&str> = old.file_symbols.classes.iter().map(|c| c.name.as_str()).collect();
    let new_classes: HashSet<&str> = new.file_symbols.classes.iter().map(|c| c.name.as_str()).collect();
    if old_classes != new_classes { return true; }

    let old_ifaces: HashSet<&str> = old.file_symbols.interfaces.iter().map(|i| i.name.as_str()).collect();
    let new_ifaces: HashSet<&str> = new.file_symbols.interfaces.iter().map(|i| i.name.as_str()).collect();
    if old_ifaces != new_ifaces { return true; }

    let old_enums: HashSet<&str> = old.file_symbols.enums.iter().map(|e| e.name.as_str()).collect();
    let new_enums: HashSet<&str> = new.file_symbols.enums.iter().map(|e| e.name.as_str()).collect();
    if old_enums != new_enums { return true; }

    let old_funcdefs: HashSet<&str> = old.file_symbols.funcdefs.iter().map(|f| f.name.as_str()).collect();
    let new_funcdefs: HashSet<&str> = new.file_symbols.funcdefs.iter().map(|f| f.name.as_str()).collect();
    if old_funcdefs != new_funcdefs { return true; }

    let old_typedefs: HashSet<&str> = old.file_symbols.typedefs.iter().map(|t| t.alias.as_str()).collect();
    let new_typedefs: HashSet<&str> = new.file_symbols.typedefs.iter().map(|t| t.alias.as_str()).collect();
    if old_typedefs != new_typedefs { return true; }

    let old_mixins: HashSet<&str> = old.file_symbols.mixins.iter().map(|m| m.name.as_str()).collect();
    let new_mixins: HashSet<&str> = new.file_symbols.mixins.iter().map(|m| m.name.as_str()).collect();
    if old_mixins != new_mixins { return true; }

    // Entry-point status changed — affects tree-shaking and unused detection.
    if old.file_symbols.is_entry != new.file_symbols.is_entry {
        return true;
    }

    false
}

// ─── Parse synchronisation ──────────────────────────────────────────────────
//
// After `DidChange` applies edits to the rope/tree it spawns a background
// parse task.  Request handlers (SemanticTokens, InlayHint, …) that read
// from `PARSE_CACHE` may fire **before** that task finishes, returning stale
// data with positions that no longer match the buffer.
//
// The solution is a lightweight per-URI generation counter paired with a
// `tokio::sync::watch` channel.  `DidChange` bumps the desired generation;
// the spawned task signals completion.  Handlers call `wait_for_parse`
// which returns immediately if no parse is in-flight, or awaits the watch
// channel (with a bounded timeout) otherwise.

/// Per-URI *desired* parse generation (monotonically increasing).
static PARSE_DESIRED: Lazy<DashMap<Url, u64>> = Lazy::new(DashMap::new);

/// Per-URI watch sender; the value is the last *completed* generation.
static PARSE_DONE_TX: Lazy<DashMap<Url, watch::Sender<u64>>> = Lazy::new(DashMap::new);

/// Called from the main message loop **before** spawning `parse_and_notify`.
///
/// Returns the new desired generation number that must later be passed to
/// [`mark_parse_done`].
pub fn mark_parse_pending(uri: &Url) -> u64 {
    let mut entry = PARSE_DESIRED.entry(uri.clone()).or_insert(0);
    *entry += 1;
    let generation = *entry;
    drop(entry);

    // Ensure the watch channel exists.
    PARSE_DONE_TX
        .entry(uri.clone())
        .or_insert_with(|| watch::channel(0).0);
    crate::debug_log!(
        "parse_cache::mark_parse_pending uri={}, generation={}",
        uri.path(),
        generation
    );
    generation
}

/// Called from the spawned parse task when it finishes (success **or** cancel).
///
/// Only advances the completed generation forward so that out-of-order
/// completions cannot regress the counter.
pub fn mark_parse_done(uri: &Url, generation: u64) {
    if let Some(tx) = PARSE_DONE_TX.get(uri) {
        let mut advanced = false;
        tx.send_if_modified(|current| {
            if generation > *current {
                *current = generation;
                advanced = true;
                true
            } else {
                false
            }
        });
        crate::debug_log!(
            "parse_cache::mark_parse_done uri={}, generation={}, advanced={}",
            uri.path(),
            generation,
            advanced
        );
    } else {
        crate::debug_log!(
            "parse_cache::mark_parse_done uri={}, generation={}, advanced=false, watcher_missing=true",
            uri.path(),
            generation
        );
    }
}

