//! Shared parse infrastructure used by every language.
//!
//! Extracts common patterns from `lng::jass::parse` and `lng::ass::parse`:
//!
//! * **Import resolution** — `resolve_import_directive` turns an
//!   `ImportDirective` into `(Url, DocumentLink, Diagnostic)`.
//! * **`file_symbols_to_entries`** — converts `FileSymbols` into
//!   `GlobalEntry` items for the scope resolver.
//! * **`compute_hash_for_uri`** — content hash from ROPE_MAP or disk.
//! * **`ensure_file_symbols`** — loads symbols for a dependency URI
//!   from cache or parses from disk, parameterized by `tree_sitter::Language`.
//! * **`cascade_parse_and_notify`** — the two-pass cascade loop
//!   (CascadeGuard + pending-drain + peer re-parse) with pluggable
//!   per-language parse functions.

use crate::lng::directive::ImportDirective;
use crate::lng::jass::symbol::FileSymbols;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity};
use crate::lsp::document_link::lsp::DocumentLink;
use crate::lsp::position::Position;
use crate::lsp::range::Range;
use crate::lsp::ref_map::{DeclKey, RefMap};
use crate::util::file_cache;
use crate::util::file_store::{
    drain_pending,
    send_refresh_all, CascadeGuard, FILE_STORE,
    MAX_CASCADE_PEERS, REPARSE_GUARD,
};
use crate::util::import_graph::resolve_import;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::scope_resolver::{GlobalEntry, SymbolNS, SCOPE_RESOLVER};
use crate::util::tree_map::TREE_MAP;
use lapce_xi_rope::Rope;
use std::collections::HashSet;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use url::Url;

// ─── Import resolution ──────────────────────────────────────────────────────

/// Resolve a single `//import` / `//import!` directive.
///
/// On success pushes into `imports`, `frozen_imports`, `links`;
/// on failure pushes into `diagnostics`.
pub fn resolve_import_directive(
    uri: &Url,
    imp: &ImportDirective,
    src: &[u8],
    rope: &Rope,
    imports: &mut HashSet<Url>,
    frozen_imports: &mut HashSet<Url>,
    links: &mut Vec<DocumentLink>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if imp.path.is_empty() {
        return; // cursor already emits "Missing import path"
    }

    let node = &imp.node;
    let prefix_len = if imp.frozen {
        "//import!".len()
    } else {
        "//import".len()
    };

    let node_text =
        std::str::from_utf8(&src[node.start_byte()..node.end_byte()]).unwrap_or("");
    let after_prefix = &node_text[prefix_len..];
    let ws_len = after_prefix.len() - after_prefix.trim_start().len();
    let path_start_byte = node.start_byte() + prefix_len + ws_len;
    let path_end_byte = node.start_byte() + prefix_len + ws_len + imp.path.len();

    let path_range = Range {
        start: Position::from_byte_offset(rope, path_start_byte).unwrap_or_default(),
        end: Position::from_byte_offset(rope, path_end_byte).unwrap_or_default(),
    };

    match resolve_import(uri, &imp.path) {
        Some(resolved) => {
            imports.insert(resolved.url.clone());
            if imp.frozen {
                frozen_imports.insert(resolved.url.clone());
            }
            if resolved.exists {
                links.push(DocumentLink {
                    range: path_range,
                    target: Some(resolved.url.to_string()),
                    tooltip: Some(resolved.url.to_string()),
                });
            } else {
                diagnostics.push(Diagnostic {
                    range: path_range,
                    message: format!("File not found: {}", imp.path),
                    severity: Some(DiagnosticSeverity::Error),
                    ..Default::default()
                });
            }
        }
        None => {
            diagnostics.push(Diagnostic {
                range: path_range,
                message: format!("Cannot resolve import path: {}", imp.path),
                severity: Some(DiagnosticSeverity::Error),
                ..Default::default()
            });
        }
    }
}

/// Resolve a path (e.g. from an AS `#include` directive) given its already-
/// computed range.
///
/// Pushes into `imports`, `links`, or `diagnostics`.
pub fn resolve_path_import(
    uri: &Url,
    path_text: &str,
    path_range: Range,
    imports: &mut HashSet<Url>,
    links: &mut Vec<DocumentLink>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match resolve_import(uri, path_text) {
        Some(resolved) => {
            imports.insert(resolved.url.clone());
            if resolved.exists {
                links.push(DocumentLink {
                    range: path_range,
                    target: Some(resolved.url.to_string()),
                    tooltip: Some(resolved.url.to_string()),
                });
            } else {
                diagnostics.push(Diagnostic {
                    range: path_range,
                    message: format!("File not found: {}", path_text),
                    severity: Some(DiagnosticSeverity::Error),
                    ..Default::default()
                });
            }
        }
        None => {
            diagnostics.push(Diagnostic {
                range: path_range,
                message: format!("Cannot resolve import path: {}", path_text),
                severity: Some(DiagnosticSeverity::Error),
                ..Default::default()
            });
        }
    }
}

// ─── Symbol helpers ─────────────────────────────────────────────────────────

/// Convert `FileSymbols` into `GlobalEntry` items for the scope resolver.
pub fn file_symbols_to_entries(uri: &Url, fs: &FileSymbols) -> Vec<GlobalEntry> {
    let mut entries = Vec::new();

    for f in &fs.functions {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: f.name.clone(),
            namespace: String::new(),
            ns: SymbolNS::Func,
            decl_key: 0,
            type_name: None,
            params: f
                .params
                .iter()
                .map(|p| (p.name.clone(), p.type_name.clone()))
                .collect(),
            return_type: f.return_type.clone(),
            is_constant: false,
            is_array: false,
            doc_comment: f.doc_comment.clone(),
        });
    }
    for n in &fs.natives {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: n.name.clone(),
            namespace: String::new(),
            ns: SymbolNS::Func,
            decl_key: 0,
            type_name: None,
            params: n
                .params
                .iter()
                .map(|p| (p.name.clone(), p.type_name.clone()))
                .collect(),
            return_type: n.return_type.clone(),
            is_constant: false,
            is_array: false,
            doc_comment: n.doc_comment.clone(),
        });
    }
    for g in &fs.globals {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: g.name.clone(),
            namespace: String::new(),
            ns: SymbolNS::Var,
            decl_key: 0,
            type_name: g.type_name.clone(),
            params: vec![],
            return_type: None,
            is_constant: g.is_constant,
            is_array: g.is_array,
            doc_comment: g.doc_comment.clone(),
        });
    }
    for t in &fs.types {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: t.name.clone(),
            namespace: String::new(),
            ns: SymbolNS::Var,
            decl_key: 0,
            type_name: None,
            params: vec![],
            return_type: None,
            is_constant: false,
            is_array: false,
            doc_comment: t.doc_comment.clone(),
        });
    }

    entries
}


/// Ensure that `FILE_SYMBOLS` and `SCOPE_RESOLVER` have entries for `dep_uri`.
///
/// **Must NOT be called for the file currently being parsed** — the caller
/// must exclude `current_uri` from the loop to avoid clobbering in-flight data.
///
/// `ts_language` is the tree-sitter language used to parse the dependency
/// when it must be read from disk (e.g. `tree_sitter_jass::language()`).
///
/// Returns `true` if symbols were successfully loaded, `false` otherwise.
///
/// ## Staleness checks
///
/// 1. **In-memory** (`FILE_STORE`) — cheapest, no I/O.
/// 2. **Disk cache** (`file_cache`) — one `stat()` call to validate
///    `FileMeta` (size + mtime).  If fresh, a partial `ParseSnapshot` is
///    reconstructed from the cached data.
/// 3. **Parse from disk** — last resort, reads + parses the file.
pub fn ensure_file_symbols(dep_uri: &Url, ts_language: tree_sitter::Language) -> bool {
    // Already fully parsed — FILE_STORE has everything.
    if FILE_STORE.contains_key(dep_uri) {
        return true;
    }

    // Disk cache — if file metadata matches, reconstruct a partial snapshot.
    if let Some(cached) = file_cache::load_if_fresh(dep_uri) {
        let entries = file_symbols_to_entries(dep_uri, &cached.symbols);
        SCOPE_RESOLVER.update_file(dep_uri, cached.content_hash, entries);

        let snapshot = std::sync::Arc::new(crate::util::file_store::ParseSnapshot {
            folding: Vec::new(),
            symbols: Vec::new(),
            semantic: std::sync::RwLock::new(Default::default()),
            diagnostics: Vec::new(),
            links: Vec::new(),
            ref_map: cached.ref_map,
            file_symbols: cached.symbols,
            _type_map: Default::default(),
            type_hints: Vec::new(),
            func_decl_keys: cached.func_decl_keys,
        });
        FILE_STORE.insert(dep_uri.clone(), snapshot);
        return true;
    }

    // Parse from disk.
    let path = match dep_uri.to_file_path() {
        Ok(p) if p.exists() => p,
        _ => return false,
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&ts_language).is_err() {
        return false;
    }
    let tree = match parser.parse(&content, None) {
        Some(t) => t,
        None => return false,
    };

    // Use the JASS AST builder as the default for dependency symbols.
    let rope = Rope::from(content.as_str());
    let ast = crate::lng::jass::ast::build_ast(tree.root_node());
    let cursor = crate::lng::jass::cursor::Cursor::walk(&ast, &rope, &[]);
    let file_symbols = cursor.file_symbols;
    let hash = file_cache::content_hash(&rope);

    // Build a full RefMap so that go-to-definition can find declaration
    // positions and `find_decl_key_by_name` can resolve the real DeclKey.
    let func_decl_keys = cursor.func_decl_keys;
    let ref_map = crate::lsp::ref_map::build_ref_map(
        cursor.ref_groups,
        cursor.ref_names,
        cursor.external_decls,
        &rope,
    );

    // Build and store a full ParseSnapshot — single source of truth.
    let snapshot = std::sync::Arc::new(crate::util::file_store::ParseSnapshot {
        folding: cursor.folding,
        symbols: cursor.symbols,
        semantic: std::sync::RwLock::new(cursor.semantic),
        diagnostics: cursor.diagnostics,
        links: vec![],
        ref_map,
        file_symbols: file_symbols.clone(),
        _type_map: cursor.type_map,
        type_hints: cursor.type_hints,
        func_decl_keys: func_decl_keys.clone(),
    });

    // Persist to unified disk cache.
    if let Some(meta) = file_cache::FileMeta::from_uri(dep_uri) {
        file_cache::store(
            dep_uri,
            meta,
            hash,
            &file_symbols,
            &snapshot.ref_map,
            &func_decl_keys,
        );
    }

    FILE_STORE.insert(dep_uri.clone(), snapshot);
    let entries = file_symbols_to_entries(dep_uri, &file_symbols);
    SCOPE_RESOLVER.update_file(dep_uri, hash, entries);
    true
}

/// Look up the `DeclKey` of a symbol by name **and namespace** in a `RefMap`.
pub fn find_decl_key_by_name(
    ref_map: &RefMap,
    name: &str,
    ns: SymbolNS,
    func_keys: &HashSet<DeclKey>,
) -> Option<DeclKey> {
    for (&key, group) in &ref_map.groups {
        if group.name != name || !group.occurrences.iter().any(|o| o.is_decl) {
            continue;
        }
        let is_func = func_keys.contains(&key);
        match ns {
            SymbolNS::Func if is_func => return Some(key),
            SymbolNS::Var if !is_func => return Some(key),
            _ => continue,
        }
    }
    None
}

// ─── Cascade ────────────────────────────────────────────────────────────────

/// Type alias for async parse functions passed to the cascade helper.
pub type ParseFn = Box<
    dyn Fn(Url) -> Pin<Box<dyn Future<Output = Result<Vec<Url>, Box<dyn Error + Send + Sync>>> + Send>>
        + Send
        + Sync,
>;

/// Two-pass cascade: parse the current file, drain pending waiters,
/// re-parse affected peers, refresh editors.
///
/// Diagnostics use the **push** model: after all parsing is done,
/// `send_refresh_all` sends `textDocument/publishDiagnostics` for every
/// file in `FILE_STORE` — both open and closed tabs see errors immediately.
///
/// # Arguments
///
/// * `uri` — the file that was just edited.
/// * `parse_fn` — language-specific parse that returns the cascade peer list.
/// * `parse_from_disk_fn` — language-specific disk parse for closed peers.
///
/// The cascade logic (CascadeGuard, pending-drain, peer loop, refresh)
/// is identical for every language.
pub async fn cascade_parse_and_notify(
    uri: &Url,
    parse_fn: &ParseFn,
    parse_from_disk_fn: Option<&ParseFn>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Cycle guard.
    let _guard = match CascadeGuard::try_enter(uri) {
        Some(g) => g,
        None => {
            log::debug!("cascade: skipping {} (already in progress)", uri.path());
            return Ok(());
        }
    };

    // Pass 1: parse the current file.
    let mut cascade = parse_fn(uri.clone()).await?;

    // Drain pending-import waiters.
    let pending_waiters = drain_pending(uri);
    for waiter in pending_waiters {
        if !cascade.contains(&waiter) {
            cascade.push(waiter);
        }
    }

    // Pass 2: cascade re-parse affected peers.
    let mut count = 0usize;

    for peer_uri in &cascade {
        if count >= MAX_CASCADE_PEERS {
            log::warn!(
                "cascade: capped at {} peers for {}",
                MAX_CASCADE_PEERS,
                uri.path()
            );
            break;
        }

        if REPARSE_GUARD.contains(peer_uri) {
            log::debug!("cascade: skipping {} (guarded)", peer_uri.path());
            continue;
        }

        let ok = if ROPE_MAP.contains_key(peer_uri) && TREE_MAP.contains_key(peer_uri) {
            match parse_fn(peer_uri.clone()).await {
                Ok(_) => true,
                Err(e) => {
                    log::error!("cascade re-parse {}: {}", peer_uri, e);
                    false
                }
            }
        } else if let Some(disk_fn) = parse_from_disk_fn {
            match disk_fn(peer_uri.clone()).await {
                Ok(_) => true,
                Err(e) => {
                    log::debug!("cascade disk-parse {}: {}", peer_uri.path(), e);
                    false
                }
            }
        } else {
            false
        };

        if ok {
            count += 1;
        }
    }

    send_refresh_all().await;

    Ok(())
}

