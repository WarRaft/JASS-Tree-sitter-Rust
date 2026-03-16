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
use crate::lng::jass::symbol::{FileSymbols, FILE_SYMBOLS};
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity};
use crate::lsp::document_link::lsp::DocumentLink;
use crate::lsp::position::Position;
use crate::lsp::range::Range;
use crate::lsp::ref_map::{DeclKey, RefMap};
use crate::util::file_store::{
    drain_pending, publish_diagnostics, publish_diagnostics_many,
    send_refresh_all, CascadeGuard,
    MAX_CASCADE_PEERS, REPARSE_GUARD,
};
use crate::util::import_graph::resolve_import;
use crate::util::ref_cache;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::scope_resolver::{GlobalEntry, SymbolNS, SCOPE_RESOLVER};
use crate::util::symbol_cache;
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
        });
    }
    for n in &fs.natives {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: n.name.clone(),
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
        });
    }
    for g in &fs.globals {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: g.name.clone(),
            ns: SymbolNS::Var,
            decl_key: 0,
            type_name: g.type_name.clone(),
            params: vec![],
            return_type: None,
            is_constant: g.is_constant,
            is_array: g.is_array,
        });
    }
    for t in &fs.types {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: t.name.clone(),
            ns: SymbolNS::Var,
            decl_key: 0,
            type_name: None,
            params: vec![],
            return_type: None,
            is_constant: false,
            is_array: false,
        });
    }

    entries
}

/// Compute content hash for a URI — reads from ROPE_MAP if available,
/// otherwise reads from disk.
#[allow(dead_code)]
pub fn compute_hash_for_uri(uri: &Url) -> [u8; 32] {
    if let Some(rope_entry) = ROPE_MAP.get(uri) {
        return ref_cache::content_hash(rope_entry.value());
    }
    if let Ok(path) = uri.to_file_path() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            let rope = Rope::from(content.as_str());
            return ref_cache::content_hash(&rope);
        }
    }
    [0u8; 32]
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
/// 1. **In-memory** (`FILE_SYMBOLS`) — cheapest, no I/O.
/// 2. **Disk cache** (`symbol_cache`) — one `stat()` call to validate
///    `FileMeta` (size + mtime).  If fresh, the stored `content_hash` is
///    reused so we never read the file just for SHA-256.
/// 3. **Parse from disk** — last resort, reads + parses the file.
pub fn ensure_file_symbols(dep_uri: &Url, ts_language: tree_sitter::Language) -> bool {
    // Already in memory — no need to reload.
    if FILE_SYMBOLS.contains_key(dep_uri) {
        return true;
    }

    // Disk cache — but only if the file hasn't changed since the cache was written.
    if let Some((cached_meta, cached_hash, symbols)) = symbol_cache::load(dep_uri) {
        let current_meta = symbol_cache::FileMeta::from_uri(dep_uri);
        if current_meta == Some(cached_meta) {
            // Cache is fresh — use the stored content_hash directly
            // instead of re-reading the file for SHA-256.
            let entries = file_symbols_to_entries(dep_uri, &symbols);
            SCOPE_RESOLVER.update_file(dep_uri, cached_hash, entries);
            FILE_SYMBOLS.insert(dep_uri.clone(), symbols);
            return true;
        }
        // Cache is stale — fall through to parse from disk.
        log::debug!(
            "ensure_file_symbols: stale cache for {}",
            dep_uri.path()
        );
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
    // This works because cross-file dependencies are always JASS files
    // (common.j, blizzard.j, etc.).  When AS gains its own FileSymbols
    // builder this branch can be extended with a language check.
    let rope = Rope::from(content.as_str());
    let ast = crate::lng::jass::ast::build_ast(tree.root_node());
    let cursor = crate::lng::jass::cursor::Cursor::walk(&ast, &rope, &[]);
    let file_symbols = cursor.file_symbols;
    let hash = ref_cache::content_hash(&rope);

    if let Some(meta) = symbol_cache::FileMeta::from_uri(dep_uri) {
        symbol_cache::store(dep_uri, meta, hash, &file_symbols);
    }
    let entries = file_symbols_to_entries(dep_uri, &file_symbols);
    SCOPE_RESOLVER.update_file(dep_uri, hash, entries);
    FILE_SYMBOLS.insert(dep_uri.clone(), file_symbols);
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
/// re-parse affected peers, push diagnostics, refresh editors.
///
/// # Arguments
///
/// * `uri` — the file that was just edited.
/// * `parse_fn` — language-specific parse that returns the cascade peer list.
/// * `parse_from_disk_fn` — language-specific disk parse for closed peers.
///
/// The cascade logic (CascadeGuard, pending-drain, peer loop, diagnostics)
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

    // Publish diagnostics for the current file right away.
    publish_diagnostics(uri).await;

    // Drain pending-import waiters.
    let pending_waiters = drain_pending(uri);
    for waiter in pending_waiters {
        if !cascade.contains(&waiter) {
            cascade.push(waiter);
        }
    }

    // Pass 2: cascade re-parse affected peers.
    let mut all_affected: Vec<Url> = Vec::new();
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
            all_affected.push(peer_uri.clone());
            count += 1;
        }
    }

    publish_diagnostics_many(&all_affected).await;
    send_refresh_all().await;

    Ok(())
}

