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

use dashmap::DashMap;
use once_cell::sync::Lazy;
use tokio::io::Stdout;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::lng::jass::symbol::FileSymbols;
use crate::lsp::diagnostic::lsp::{DocumentDiagnosticReport, Diagnostic};
use crate::lsp::document_link::lsp::DocumentLink;
use crate::lsp::document_symbol::lsp::DocumentSymbol;
use crate::lsp::folding::lsp::FoldingRange;
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
    /// DeclKeys that belong to function / native declarations.
    #[allow(dead_code)]
    pub func_decl_keys: HashSet<DeclKey>,
}

// ─── Global stores ───────────────────────────────────────────────────────────

/// Per-URI last-good parse snapshot.
pub static FILE_STORE: Lazy<DashMap<Url, Arc<ParseSnapshot>>> = Lazy::new(DashMap::new);

/// Per-URI cancellation token.
pub static CANCEL_TOKENS: Lazy<DashMap<Url, CancellationToken>> = Lazy::new(DashMap::new);

/// Shared writer for pushing notifications (set once from `main()`).
pub static LSP_WRITER: once_cell::sync::OnceCell<Arc<Mutex<Stdout>>> =
    once_cell::sync::OnceCell::new();

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

/// Push `textDocument/publishDiagnostics` to the client for `uri`.
///
/// Reads diagnostics from the stored snapshot.  No-op when the writer
/// hasn't been initialised yet or when no snapshot exists for `uri`.
pub async fn publish_diagnostics(uri: &Url) {
    let writer = match LSP_WRITER.get() {
        Some(w) => w,
        None => return,
    };
    let diagnostics: Vec<Diagnostic> = FILE_STORE
        .get(uri)
        .map(|s| s.diagnostics.clone())
        .unwrap_or_default();

    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri.to_string(),
            "diagnostics": diagnostics,
        }
    });

    crate::lsp::send::send(writer, &msg).await;
}

/// Push diagnostics for several URIs at once (after cascade).
pub async fn publish_diagnostics_many(uris: &[Url]) {
    for uri in uris {
        publish_diagnostics(uri).await;
    }
}

/// Ask the client to re-request semantic tokens, diagnostics, and inlay hints
/// for **all** open files.
///
/// These are server→client **requests** (not notifications) and require a
/// unique `id` — without it VS Code silently ignores them.
pub async fn send_refresh_all() {
    let writer = match LSP_WRITER.get() {
        Some(w) => w,
        None => return,
    };
    for method in [
        "workspace/semanticTokens/refresh",
        "workspace/diagnostics/refresh",
        "workspace/inlayHint/refresh",
    ] {
        crate::lsp::send::send_request(writer, method).await;
    }
}

/// Convenience: build a [`DocumentDiagnosticReport`] from the snapshot.
///
/// Falls back to the legacy `DIAGNOSTIC_URI_MAP` for languages (AngelScript,
/// BNI) that don't use `FILE_STORE` yet.
pub fn diagnostic_report(uri: &Url) -> DocumentDiagnosticReport {
    if let Some(snap) = FILE_STORE.get(uri) {
        return DocumentDiagnosticReport::Full {
            result_id: None,
            items: snap.diagnostics.clone(),
            related_documents: None,
        };
    }

    // Fallback: legacy per-feature DashMap (AngelScript / BNI).
    use crate::lsp::diagnostic::uri_map::URI_MAP as DIAGNOSTIC_URI_MAP;
    if let Some(report) = DIAGNOSTIC_URI_MAP.get(uri) {
        return report.value().clone();
    }

    DocumentDiagnosticReport::Full {
        result_id: None,
        items: vec![],
        related_documents: None,
    }
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

    false
}

