//! Central per-file storage — replaces scattered per-feature DashMaps.
//!
//! **`FILE_STORE`** holds an `Arc<ParseSnapshot>` per URI.  A snapshot is
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
use std::time::Duration;

use dashmap::{DashMap, DashSet};
use log::info;
use once_cell::sync::Lazy;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::lng::jass::symbol::FileSymbols;
use crate::lng::jass::type_map::TypeMap;
use crate::lsp::color::lsp::ColorInformation;
use crate::lsp::diagnostic::lsp::Diagnostic;
use crate::lsp::document_link::lsp::DocumentLink;
use crate::lsp::document_symbol::lsp::DocumentSymbol;
use crate::lsp::folding::lsp::FoldingRange;
use crate::lsp::inlay_hint::lsp::InlayHint;
use crate::lsp::ref_map::{DeclKey, RefMap};
use crate::lsp::semantic::hub::Hub;
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
    /// Inlay hints for type annotations (shown when `//set type-tip 1`).
    pub type_hints: Vec<InlayHint>,
    /// Inlay hints from `//import-ujapi!` — always visible (version tag).
    pub ujapi_hints: Vec<InlayHint>,
    /// DeclKeys that belong to function / native declarations.
    pub func_decl_keys: HashSet<DeclKey>,
    /// Color information for `|cAARRGGBB` in strings and `0xAARRGGBB` hex literals.
    pub colors: Vec<ColorInformation>,
}

// ─── Global stores ───────────────────────────────────────────────────────────

/// Per-URI last-good parse snapshot.
pub static FILE_STORE: Lazy<DashMap<Url, Arc<ParseSnapshot>>> = Lazy::new(DashMap::new);


/// Per-URI cancellation token.
pub static CANCEL_TOKENS: Lazy<DashMap<Url, CancellationToken>> = Lazy::new(DashMap::new);

/// ─── Per-URI request cancellation ────────────────────────────────────────────
///
/// When `textDocument/didChange` arrives, ALL in-flight LSP request handlers for
/// the same URI are stale — the client will discard their responses anyway.
/// We keep a single `CancellationToken` per URI that request handlers poll;
/// `cancel_uri_requests` replaces it with a fresh one, instantly cancelling
/// every handler that captured the old token.
///
/// Per-URI cancellation token for in-flight LSP request handlers.
static REQUEST_TOKENS: Lazy<DashMap<Url, CancellationToken>> = Lazy::new(DashMap::new);

/// Cancel all in-flight request handlers for `uri` and install a fresh token.
///
/// Called from the `DidChange` handler on the main loop, **before** applying
/// edits — so handlers that are already spawned see `is_cancelled()` immediately.
pub fn cancel_uri_requests(uri: &Url) {
    if let Some(old) = REQUEST_TOKENS.get(uri) {
        old.cancel();
    }
    REQUEST_TOKENS.insert(uri.clone(), CancellationToken::new());
}

/// Get (or create) the current request cancellation token for `uri`.
///
/// Spawned request handlers call this once at the start to obtain a token
/// they can poll with `is_cancelled()` or race with `cancelled().await`.
pub fn uri_request_token(uri: &Url) -> CancellationToken {
    REQUEST_TOKENS
        .entry(uri.clone())
        .or_insert_with(CancellationToken::new)
        .clone()
}

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
    if let Some(old) = CANCEL_TOKENS.get(uri) {
        old.cancel();
    }
    let token = CancellationToken::new();
    CANCEL_TOKENS.insert(uri.clone(), token.clone());
    token
}

/// Push a unified `custom/parseResult` notification for **every** file in
/// `FILE_STORE`.
pub async fn send_refresh_all() {
    push_parse_results().await;
}

/// Push a unified `custom/parseResult` for a single URI.
pub async fn push_parse_result_for_uri(uri: &Url) {
    let data = match build_parse_result(uri) {
        Some(d) => d,
        None => return,
    };

    crate::lsp::send::send(&data).await;
}

/// Build the JSON notification payload for one URI.
fn build_parse_result(uri: &Url) -> Option<serde_json::Value> {
    use crate::lsp::inlay_hint::send::compute_all;
    use serde_json::json;

    let snapshot = FILE_STORE.get(uri)?;
    let snap = snapshot.value();

    let semantic_data = snap.semantic.read().unwrap().data(None);
    let diagnostics = &snap.diagnostics;
    let hints = compute_all(uri);
    let folding = &snap.folding;
    let symbols = &snap.symbols;
    let links = &snap.links;
    let colors = &snap.colors;

    Some(json!({
        "jsonrpc": "2.0",
        "method": "custom/parseResult",
        "params": {
            "uri": uri.to_string(),
            "semanticTokens": semantic_data,
            "diagnostics": diagnostics,
            "inlayHints": hints,
            "folding": folding,
            "symbols": symbols,
            "documentLinks": links,
            "colors": colors
        }
    }))
}

/// Push `custom/parseResult` for every file in `FILE_STORE`.
///
/// **Important**: we snapshot the data first and drop the DashMap guards
/// *before* awaiting any IO.  Holding a DashMap read-lock across `.await`
/// would deadlock with concurrent `insert()` calls from parse tasks.
async fn push_parse_results() {
    use crate::lsp::inlay_hint::send::compute_all;
    use serde_json::json;

    let payloads: Vec<serde_json::Value> = FILE_STORE
        .iter()
        .filter_map(|entry| {
            let uri = entry.key();
            let snap = entry.value();

            let semantic_data = snap.semantic.read().unwrap().data(None);
            let diagnostics = snap.diagnostics.clone();
            let hints = compute_all(uri);
            let folding = snap.folding.clone();
            let symbols = snap.symbols.clone();
            let links = snap.links.clone();
            let colors = snap.colors.clone();

            Some(json!({
                "jsonrpc": "2.0",
                "method": "custom/parseResult",
                "params": {
                    "uri": uri.to_string(),
                    "semanticTokens": semantic_data,
                    "diagnostics": diagnostics,
                    "inlayHints": hints,
                    "folding": folding,
                    "symbols": symbols,
                    "documentLinks": links,
                    "colors": colors
                }
            }))
        })
        .collect();
    // DashMap guards dropped here.

    for payload in &payloads {
        crate::lsp::send::send(payload).await;
    }
}

// ─── DidClose cleanup ────────────────────────────────────────────────────────

/// Evict per-editor state for a closed file and, if the whole import tree
/// has no more open files, evict the tree's `FILE_STORE` / scope entries
/// so that memory is reclaimed.
///
/// Returns the set of URIs whose diagnostics were cleared (caller should
/// send empty `publishDiagnostics` for each).
pub fn evict_closed_file(uri: &Url) -> Vec<Url> {
    use crate::util::import_graph::IMPORT_GRAPH;
    use crate::util::roper::uri_map::ROPE_MAP;
    use crate::util::scope_resolver::SCOPE_RESOLVER;
    use crate::util::tree_map::{PARSER_MAP, TREE_MAP};
    use crate::util::uri_map::LNG_URI_MAP;

    // 1. Remove per-editor state for the closed file.
    ROPE_MAP.remove(uri);
    TREE_MAP.remove(uri);
    PARSER_MAP.remove(uri);
    LNG_URI_MAP.remove(uri);
    CANCEL_TOKENS.remove(uri);
    REQUEST_TOKENS.remove(uri);
    PARSE_DESIRED.remove(uri);
    PARSE_DONE_TX.remove(uri);

    // 2. Determine the import-tree the closed file belongs to.
    let tree_uris = IMPORT_GRAPH.tree_for_uri(uri);

    // 3. Check if any file in that tree is still open in the editor.
    let any_open = tree_uris.iter().any(|u| ROPE_MAP.contains_key(u));

    if any_open {
        // At least one file is still open — keep FILE_STORE intact.
        return vec![];
    }

    // 4. No open files remain in this tree — evict everything.
    let mut evicted: Vec<Url> = Vec::with_capacity(tree_uris.len());
    for tree_uri in &tree_uris {
        if FILE_STORE.remove(tree_uri).is_some() {
            evicted.push(tree_uri.clone());
        }
        CANCEL_TOKENS.remove(tree_uri);
        REQUEST_TOKENS.remove(tree_uri);
        PARSE_DESIRED.remove(tree_uri);
        PARSE_DONE_TX.remove(tree_uri);
    }

    SCOPE_RESOLVER.remove_files(&tree_uris);

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
pub fn is_uri_frozen(target_uri: &Url) -> bool {
    FILE_STORE.iter().any(|entry| {
        entry.value().file_symbols.frozen_imports.contains(target_uri)
    })
}

/// Check if `uri` is marked as an **entry point** (contains `//entry` directive).
pub fn is_uri_entry(uri: &Url) -> bool {
    FILE_STORE
        .get(uri)
        .map(|snap| snap.file_symbols.is_entry)
        .unwrap_or(false)
}

/// Collect all URIs that are marked as entry points (`//entry` directive).
pub fn entry_uris() -> Vec<Url> {
    FILE_STORE
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
// from `FILE_STORE` may fire **before** that task finishes, returning stale
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
    generation
}

/// Called from the spawned parse task when it finishes (success **or** cancel).
///
/// Only advances the completed generation forward so that out-of-order
/// completions cannot regress the counter.
pub fn mark_parse_done(uri: &Url, generation: u64) {
    if let Some(tx) = PARSE_DONE_TX.get(uri) {
        tx.send_if_modified(|current| {
            if generation > *current {
                *current = generation;
                true
            } else {
                false
            }
        });
    }
}

/// Returns `true` if a parse for `uri` has been requested but not yet completed.
///
/// Used by [`cascade_parse_and_notify`] to suppress spurious
/// `workspace/semanticTokens/refresh` calls from cancelled parse tasks that
/// have already been superseded by a newer one.
pub fn is_parse_in_flight(uri: &Url) -> bool {
    let desired = match PARSE_DESIRED.get(uri) {
        Some(v) => *v,
        None => return false,
    };
    let done = match PARSE_DONE_TX.get(uri) {
        Some(tx) => *tx.borrow(),
        None => return false,
    };
    desired > done
}


/// Like [`wait_for_parse`], but also aborts early if `cancel` fires.
///
/// Returns `true` if the parse completed normally, `false` if the token
/// was cancelled (meaning a new `didChange` arrived and this request is stale).
pub async fn wait_for_parse_cancellable(
    uri: &Url,
    timeout: Duration,
    cancel: &CancellationToken,
) -> bool {
    if cancel.is_cancelled() {
        return false;
    }

    let mut rx = match PARSE_DONE_TX.get(uri) {
        Some(tx) => tx.subscribe(),
        None => return true, // no parse pending — proceed
    };

    let uri = uri.clone();
    let cancel = cancel.clone();

    let result = tokio::time::timeout(timeout, async {
        loop {
            if cancel.is_cancelled() {
                return false;
            }
            let desired = match PARSE_DESIRED.get(&uri) {
                Some(v) => *v,
                None => return true,
            };
            if *rx.borrow() >= desired {
                return true;
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return false,
                res = rx.changed() => {
                    if res.is_err() { return true; }
                }
            }
        }
    })
    .await;

    match result {
        Ok(completed) => completed,
        Err(_) => true, // timeout — proceed with what we have
    }
}
