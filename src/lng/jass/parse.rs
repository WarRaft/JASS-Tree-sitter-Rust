use crate::lng::jass::ast::{build_ast, rewrite_imports, Statement};
use crate::lng::jass::cursor::{Cursor, ImportedKind, ImportedSymbol};
use crate::lng::jass::symbol::FILE_SYMBOLS;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity};
use crate::lsp::ref_map::{build_ref_map, RefMap, REF_URI_MAP};
use crate::util::file_store::{
    exports_changed, new_cancel_token, register_pending,
    ParseSnapshot, FILE_STORE,
};
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::parse::{
    cascade_parse_and_notify, ensure_file_symbols,
    file_symbols_to_entries, find_decl_key_by_name, resolve_import_directive,
    ParseFn,
};
use crate::util::ref_cache;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::scope_resolver::{SymbolNS, SCOPE_RESOLVER};
use crate::util::symbol_cache;
use crate::util::tree_map::TREE_MAP;
use lapce_xi_rope::Rope;
use std::collections::HashSet;
use std::error::Error;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use url::Url;

// ─── Main parse entry point ─────────────────────────────────────────────────


/// Parse and store all LSP data for `uri`.
///
/// The heavy `_parse` work runs on the blocking thread pool so that tokio
/// worker threads stay free for I/O and other request handlers.
///
/// Returns the list of peer URIs that should be cascade-re-parsed.
pub async fn parse(uri: &Url) -> Result<Vec<Url>, Box<dyn Error + Send + Sync>> {
    let token = new_cancel_token(uri);

    // ── Clone owned data from DashMap guards and drop the guards immediately ──
    let rope = ROPE_MAP
        .get(uri)
        .map(|r| r.value().clone())
        .ok_or("no rope")?;
    let tree = TREE_MAP
        .get(uri)
        .map(|t| t.value().clone())
        .ok_or("no tree")?;

    let uri_owned = uri.clone();
    let cancel = token.clone();

    // ── Run CPU-heavy parse on the blocking thread pool ──
    let handle = tokio::task::spawn_blocking(move || _parse(&uri_owned, &rope, &tree, &cancel));

    // ── Race the blocking work against cancellation ──
    tokio::select! {
        res = handle => res.map_err(|e| -> Box<dyn Error + Send + Sync> { e.into() })?,
        _ = token.cancelled() => Ok(vec![]),
    }
}

/// Parse + cascade re-parse + push diagnostics + refresh all open editors.
///
/// ## Two-pass design
///
/// **Pass 1 — current file**: The edited file is parsed from its in-memory
/// rope/tree (the latest snapshot from `DidChange`).  All local declarations
/// are collected, unresolved references are queued, and the import graph is
/// updated.  Any dependency that cannot be loaded triggers
/// [`register_pending`] so that a cascade fires when it becomes available.
///
/// **Pass 2 — cascade**: If the current file's **exports** changed (names
/// added/removed), or if the connected component changed, every peer in the
/// component is re-parsed so its diagnostics and references reflect the new
/// scope.  Peers that are **open** (have rope/tree) are re-parsed in-memory;
/// **closed** peers are re-parsed from disk.
///
/// Cyclic cascades are prevented by [`CascadeGuard`] (RAII set guard).
///
/// Intended to be called from a **spawned task** (not the main message loop).
pub async fn parse_and_notify(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parse_fn: ParseFn = Box::new(|u| {
        Box::pin(async move { parse(&u).await })
    });
    let disk_fn: ParseFn = Box::new(|u| {
        Box::pin(async move { parse_from_disk(&u).await })
    });
    cascade_parse_and_notify(uri, &parse_fn, Some(&disk_fn)).await
}

/// Core parse logic (runs on the blocking thread pool).
///
/// ## Two-pass reference resolution
///
/// ### Pass 1 — Local (inside [`Cursor::walk`])
///
/// The AST is walked top-to-bottom.  Every declaration creates a
/// [`DeclKey`] in the highlight scopes.  References that resolve within
/// the current file's scopes are linked immediately.  Those that don't
/// (forward references like `B = 3` before `boolean B`, or cross-file
/// symbols like `CreateUnit` from `common.j`) are collected into
/// `unresolved_refs`.
///
/// ### Pass 2 — Import linking (inside [`Cursor::link_imports`])
///
/// Each `(name, namespace)` pair from `unresolved_refs` is matched:
///
/// 1. **Forward local** — if the global scope now contains a declaration
///    for `name` (created after the reference in Phase 1), merge.
///
/// 2. **Imported symbol** — if `name` exists in the connected component's
///    scope (from `imported_symbols`), create an external ref group.
///
/// 3. **Truly unresolved** — emit `"Undeclared"` diagnostic.
///
/// This guarantees exactly two passes over references, with no
/// redundant re-scans.
fn _parse(
    uri: &Url,
    rope: &Rope,
    tree: &tree_sitter::Tree,
    cancel: &CancellationToken,
) -> Result<Vec<Url>, Box<dyn Error + Send + Sync>> {
    let root = tree.root_node();

    // 1. Build AST from CST
    let mut ast = build_ast(root);

    // ── Cancellation checkpoint ──
    if cancel.is_cancelled() { return Ok(vec![]); }

    // 2. Rewrite leading `//import` / `//import!` comments into Import nodes
    let src: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
    rewrite_imports(&mut ast, &src);

    // 3. Extract resolved import URLs, document links, and diagnostics
    //    for non-existent import paths.
    let mut imports = HashSet::new();
    let mut frozen_imports = HashSet::new();
    let mut links = Vec::new();
    let mut import_diagnostics = Vec::new();

    for item in &ast.items {
        if let Statement::Import(imp) = item {
            resolve_import_directive(
                uri, imp, &src, rope,
                &mut imports, &mut frozen_imports, &mut links, &mut import_diagnostics,
            );
        }
    }

    // ── Cancellation checkpoint ──
    if cancel.is_cancelled() { return Ok(vec![]); }

    // Capture the old connected component BEFORE updating the graph,
    // so we can detect peers that lost visibility after import removal.
    let old_component = IMPORT_GRAPH.connected_component(uri);

    IMPORT_GRAPH.update(uri, imports.clone());

    // 4. Gather symbols from the **entire connected component** (unified scope).
    let mut imported_symbols: Vec<ImportedSymbol> = Vec::new();
    let component: HashSet<Url>;
    {
        component = IMPORT_GRAPH.connected_component(uri);

        // Ensure every PEER is in the scope resolver.
        // Skip `uri` itself to avoid clobbering current-file data with stale cache.
        for peer_uri in &component {
            if peer_uri != uri {
                if !ensure_file_symbols(peer_uri, tree_sitter_jass::language().into()) {
                    // Dependency not available yet — register so that when it
                    // finishes parsing we get a cascade re-parse.
                    register_pending(peer_uri, uri);
                    log::info!(
                        "pending import: {} waits for {}",
                        uri.path(),
                        peer_uri.path()
                    );
                }
            }
        }

        // O(1)-per-name: get all symbols from the connected component.
        let visible_entries = SCOPE_RESOLVER.all_visible(&component);

        for entry in &visible_entries {
            // Skip entries from the file being parsed — cursor builds them locally.
            if &entry.uri == uri {
                continue;
            }
            let origin_snapshot = FILE_STORE.get(&entry.uri);
            let origin_decl_key = origin_snapshot.as_ref().and_then(|snap| {
                find_decl_key_by_name(
                    &snap.ref_map,
                    &entry.name,
                    entry.ns,
                    &snap.func_decl_keys,
                )
            });

            imported_symbols.push(ImportedSymbol {
                origin_uri: entry.uri.clone(),
                name: entry.name.clone(),
                kind: match entry.ns {
                    SymbolNS::Func => ImportedKind::Func,
                    SymbolNS::Var => ImportedKind::Var,
                },
                origin_decl_key,
            });
        }
    }

    // ── Cancellation checkpoint ──
    if cancel.is_cancelled() { return Ok(vec![]); }

    // 5. Single-pass cursor: diagnostics + symbols + folding + id_roles + scopes
    let mut cursor = Cursor::walk(&ast, rope, &imported_symbols);
    cursor.file_symbols.frozen_imports = frozen_imports;
    cursor.file_symbols.file_settings = cursor.file_settings.clone();
    cursor.file_symbols.bare_callees = cursor.bare_callees.clone();

    // 6. Merge import diagnostics with cursor diagnostics
    let mut all_diagnostics = cursor.diagnostics;
    all_diagnostics.extend(import_diagnostics);

    // 7. Build ref_map + persist to disk cache.
    let func_decl_keys = cursor.func_decl_keys;
    let ref_map = build_ref_map(cursor.ref_groups, cursor.ref_names, cursor.external_decls, rope);
    let hash = ref_cache::content_hash(rope);
    ref_cache::store(uri, &hash, &ref_map);

    // 8. Call-graph diagnostics: unused functions & cyclic calls.
    //    We need FILE_SYMBOLS populated for the current file, so write it first.
    FILE_SYMBOLS.insert(uri.clone(), cursor.file_symbols.clone());
    {
        use crate::util::call_graph::diagnose_functions;

        let func_diag = diagnose_functions(uri);

        for (&key, group) in &ref_map.groups {
            if !func_decl_keys.contains(&key) {
                continue;
            }
            if let Some(decl_occ) = group.occurrences.iter().find(|o| o.is_decl) {
                if func_diag.unused.contains(&group.name) {
                    all_diagnostics.push(Diagnostic {
                        range: decl_occ.range.clone(),
                        message: format!("Unused function `{}`", group.name),
                        severity: Some(DiagnosticSeverity::Hint),
                        tags: Some(vec![crate::lsp::diagnostic::lsp::DiagnosticTag::Unnecessary]),
                        ..Default::default()
                    });
                }
                if func_diag.in_cycle.contains(&group.name) {
                    all_diagnostics.push(Diagnostic {
                        range: decl_occ.range.clone(),
                        message: format!(
                            "Function `{}` is part of a cyclic call chain — cannot be ordered",
                            group.name
                        ),
                        severity: Some(DiagnosticSeverity::Warning),
                        ..Default::default()
                    });
                }
            }
        }
    }

    // ── FINAL cancellation check — don't store stale results ──
    if cancel.is_cancelled() { return Ok(vec![]); }

    // 9. Build snapshot and store atomically.
    let old_snapshot = FILE_STORE.get(uri).map(|e| Arc::clone(e.value()));

    let new_snapshot = Arc::new(ParseSnapshot {
        folding: cursor.folding,
        symbols: cursor.symbols,
        semantic: std::sync::RwLock::new(cursor.semantic),
        diagnostics: all_diagnostics,
        links,
        ref_map,
        file_symbols: cursor.file_symbols,
        type_map: cursor.type_map,
        type_hints: cursor.type_hints,
        func_decl_keys,
    });

    // Persist to disk cache.
    if let Some(meta) = symbol_cache::FileMeta::from_uri(uri) {
        symbol_cache::store(uri, meta, &new_snapshot.file_symbols);
    }

    // Keep FILE_SYMBOLS / REF_URI_MAP in sync for cross-file consumers.
    FILE_SYMBOLS.insert(uri.clone(), new_snapshot.file_symbols.clone());
    REF_URI_MAP.insert(uri.clone(), RefMap {
        groups: new_snapshot.ref_map.groups.clone(),
        spans: new_snapshot.ref_map.spans.clone(),
        external_decls: new_snapshot.ref_map.external_decls.clone(),
    });

    // ── Atomic store ──
    FILE_STORE.insert(uri.clone(), new_snapshot.clone());

    // 10. Export diff — decide on cascade.
    let did_change = exports_changed(old_snapshot.as_deref(), &new_snapshot);
    {
        let entries = file_symbols_to_entries(uri, &new_snapshot.file_symbols);
        SCOPE_RESOLVER.update_file(uri, hash, entries);
    }

    let cascade = if did_change || old_component != component {
        let mut all_affected = old_component;
        all_affected.extend(component.into_iter());
        all_affected
            .into_iter()
            .filter(|peer| peer != uri)
            .collect()
    } else {
        vec![]
    };

    Ok(cascade)
}

/// Parse a **closed** file from disk, produce a full [`ParseSnapshot`], and
/// store it in [`FILE_STORE`].  Call [`publish_diagnostics`] afterwards to push
/// the result to the client.
///
/// Returns the cascade list (peer URIs whose exports may have changed).
pub async fn parse_from_disk(uri: &Url) -> Result<Vec<Url>, Box<dyn Error + Send + Sync>> {
    let path = uri.to_file_path().map_err(|_| "invalid file path")?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(&path)?;
    let rope = Rope::from(content.as_str());

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_jass::language().into())?;
    let tree = parser.parse(&content, None).ok_or("parse failed")?;

    let token = new_cancel_token(uri);
    let uri_owned = uri.clone();
    let cancel = token.clone();

    let handle = tokio::task::spawn_blocking(move || _parse(&uri_owned, &rope, &tree, &cancel));

    let result = tokio::select! {
        res = handle => res.map_err(|e| -> Box<dyn Error + Send + Sync> { e.into() })?,
        _ = token.cancelled() => Ok(vec![]),
    };

    result
}

