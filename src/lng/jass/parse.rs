use crate::lng::jass::ast::{Statement, build_ast, rewrite_imports};
use crate::lng::jass::cursor::{Cursor, ImportedKind, ImportedSymbol};
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity};
use crate::lsp::ref_map::{DeclKey, RefMap, build_ref_map};
use crate::util::file_cache;
use crate::util::file_store::{
    FILE_STORE, ParseSnapshot, exports_changed, new_cancel_token, register_pending,
};
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::parse::{
    ParseFn, cascade_parse_and_notify, ensure_file_symbols, file_symbols_to_entries,
    find_decl_key_by_name, resolve_import_directive,
};
use crate::util::roper::node::NodeExt;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::scope_resolver::{SCOPE_RESOLVER, SymbolNS};
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
pub async fn parse_and_notify(
    uri: &Url,
    generation: Option<u64>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parse_fn: ParseFn = Box::new(|u| Box::pin(async move { parse(&u).await }));
    cascade_parse_and_notify(uri, &parse_fn, None, generation).await
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
    if cancel.is_cancelled() {
        return Ok(vec![]);
    }

    // 2. Rewrite leading `//import` / `//import!` comments into Import nodes
    let src: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
    rewrite_imports(&mut ast, &src);

    // 3. Extract resolved import URLs, document links, and diagnostics
    //    for non-existent import paths.
    let mut imports = HashSet::new();
    let mut frozen_imports = HashSet::new();
    let mut links = Vec::new();
    let mut import_diagnostics = Vec::new();
    let mut ujapi_hints = Vec::new();

    for item in &ast.items {
        if let Statement::Import(imp) = item {
            resolve_import_directive(
                uri,
                imp,
                &src,
                rope,
                &mut imports,
                &mut frozen_imports,
                &mut links,
                &mut import_diagnostics,
            );
        }
        if let Statement::UjapiImport(ud) = item {
            crate::util::parse::resolve_ujapi_directive(
                uri,
                ud,
                &src,
                rope,
                &mut imports,
                &mut frozen_imports,
                &mut links,
                &mut import_diagnostics,
                &mut ujapi_hints,
            );
        }
    }

    // ── Cancellation checkpoint ──
    if cancel.is_cancelled() {
        return Ok(vec![]);
    }

    // Capture the old visible component BEFORE updating the graph,
    // so we can detect peers that lost visibility after import removal.
    let old_component = IMPORT_GRAPH.visible_component(uri);

    IMPORT_GRAPH.update(uri, imports.clone());

    // Refresh entry cache so that visible_component reads the updated graph.
    IMPORT_GRAPH.recompute_entry_cache();

    // 4. Gather symbols from the **visible component** (entry-aware scope).
    let mut imported_symbols: Vec<ImportedSymbol> = Vec::new();
    let component: HashSet<Url>;
    {
        component = IMPORT_GRAPH.visible_component(uri);

        // Ensure every PEER is in the scope resolver.
        // Skip `uri` itself to avoid clobbering current-file data with stale cache.
        for peer_uri in &component {
            if peer_uri != uri {
                let ts_lang = if crate::util::open::is_as_uri(peer_uri) {
                    tree_sitter_as::language().into()
                } else {
                    tree_sitter_jass::language().into()
                };
                if !ensure_file_symbols(peer_uri, ts_lang) {
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
            // Try to get the precise DeclKey from FILE_STORE; fall back to
            // the scope resolver's `decl_key` (which is always available,
            // even when the origin file hasn't been fully parsed into FILE_STORE yet).
            let origin_snapshot = FILE_STORE.get(&entry.uri);
            let origin_decl_key = origin_snapshot
                .as_ref()
                .and_then(|snap| {
                    find_decl_key_by_name(
                        &snap.ref_map,
                        &entry.name,
                        entry.ns,
                        &snap.func_decl_keys,
                    )
                })
                .or(Some(entry.decl_key as DeclKey));

            imported_symbols.push(ImportedSymbol {
                origin_uri: entry.uri.clone(),
                name: entry.name.clone(),
                kind: match entry.ns {
                    SymbolNS::Func => ImportedKind::Func,
                    SymbolNS::Var => ImportedKind::Var,
                },
                origin_decl_key,
                return_type: entry.return_type.clone(),
                type_name: entry.type_name.clone(),
            });
        }
    }

    // ── Cancellation checkpoint ──
    if cancel.is_cancelled() {
        return Ok(vec![]);
    }

    // 5. Single-pass cursor: diagnostics + symbols + folding + id_roles + scopes
    let mut cursor = Cursor::walk(&ast, rope, &imported_symbols);
    cursor.file_symbols.frozen_imports = frozen_imports;
    cursor.file_symbols.file_settings = cursor.file_settings.clone();
    cursor.file_symbols.file_ignore_tags = cursor.file_ignore_tags.clone();
    cursor.file_symbols.bare_callees = cursor.bare_callees.clone();

    // Snapshot values needed for post-walk diagnostics (before cursor is consumed).
    let new_snapshot_is_entry = cursor.file_symbols.is_entry;
    let cursor_file_settings = cursor.file_symbols.file_settings.clone();

    // 6. Merge import diagnostics with cursor diagnostics
    let mut all_diagnostics = cursor.diagnostics;
    all_diagnostics.extend(import_diagnostics);

    // 7. Build ref_map.
    let func_decl_keys = cursor.func_decl_keys;
    let ref_map = build_ref_map(
        cursor.ref_groups,
        cursor.ref_names,
        cursor.external_decls,
        rope,
    );
    let hash = file_cache::content_hash(rope);

    // 8. Call-graph diagnostics: unused functions & cyclic calls.
    //    We need FILE_STORE populated for the current file, so write a
    //    preliminary snapshot first (it will be replaced below).
    //
    // IMPORTANT: capture the true old snapshot BEFORE inserting the preliminary.
    // If cancellation fires at the final checkpoint we restore it so that
    // FILE_STORE is never left with the empty preliminary (which would cause
    // the SemanticTokens handler to return an empty token list).
    let true_old_snapshot: Option<Arc<ParseSnapshot>> =
        FILE_STORE.get(uri).map(|e| Arc::clone(e.value()));

    let preliminary = Arc::new(ParseSnapshot {
        folding: Vec::new(),
        symbols: Vec::new(),
        semantic: std::sync::RwLock::new(Default::default()),
        diagnostics: Vec::new(),
        links: Vec::new(),
        ref_map: RefMap::default(),
        file_symbols: cursor.file_symbols.clone(),
        _type_map: Default::default(),
        type_hints: Vec::new(),
        ujapi_hints: Vec::new(),
        func_decl_keys: func_decl_keys.clone(),
        colors: Vec::new(),
    });
    FILE_STORE.insert(uri.clone(), preliminary);
    {
        use crate::util::call_graph::diagnose_functions;

        let func_diag = diagnose_functions(uri);

        // File-level `//ignore unused` suppresses all unused-function diagnostics.
        let file_unused_suppressed = cursor.file_ignore_tags.contains("unused");
        // File-level `//ignore cycle` suppresses all cyclic-call diagnostics.
        let file_cycle_suppressed = cursor.file_ignore_tags.contains("cycle");

        for (&key, group) in &ref_map.groups {
            if !func_decl_keys.contains(&key) {
                continue;
            }
            if let Some(decl_occ) = group.occurrences.iter().find(|o| o.is_decl) {
                if func_diag.unused.contains(&group.name) {
                    // Check file-level suppression and per-declaration //@ignore unused
                    let per_decl_suppressed = cursor
                        .file_symbols
                        .functions
                        .iter()
                        .any(|f| f.name == group.name && f.ignore_tags.contains("unused"));
                    if !file_unused_suppressed && !per_decl_suppressed {
                        // Find the full function range from the AST.
                        let func_range = ast.items.iter().find_map(|item| {
                            if let Statement::Function(f) = item {
                                let fname = f.name.as_ref().map(|id| {
                                    rope.slice_to_cow(id.node.start_byte()..id.node.end_byte())
                                        .to_string()
                                });
                                if fname.as_deref() == Some(&group.name) {
                                    Some(f.node.to_range(rope))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        });

                        all_diagnostics.push(Diagnostic {
                            range: decl_occ.range.clone(),
                            message: crate::util::i18n::unused_function(&group.name),
                            severity: Some(DiagnosticSeverity::Hint),
                            tags: Some(vec![
                                crate::lsp::diagnostic::lsp::DiagnosticTag::Unnecessary,
                            ]),
                            source: Some("jass".into()),
                            code: Some(crate::lsp::diagnostic::lsp::DiagnosticCode::String("unused-function".into())),
                            data: Some(serde_json::json!({
                                "unused_func_range": func_range,
                            })),
                            ..Default::default()
                        });
                    }
                }
                if func_diag.in_cycle.contains(&group.name) {
                    let per_decl_suppressed = cursor
                        .file_symbols
                        .functions
                        .iter()
                        .any(|f| f.name == group.name && f.ignore_tags.contains("cycle"));
                    if !file_cycle_suppressed && !per_decl_suppressed {
                        all_diagnostics.push(Diagnostic {
                            range: decl_occ.range.clone(),
                            message: crate::util::i18n::cyclic_call_chain(&group.name),
                            severity: Some(DiagnosticSeverity::Warning),
                            ..Diagnostic::new("jass", "cyclic-call")
                        });
                    }
                }
                if func_diag.inlinable.contains(&group.name) {
                    let per_decl_suppressed = cursor
                        .file_symbols
                        .functions
                        .iter()
                        .any(|f| f.name == group.name && f.ignore_tags.contains("inline"));
                    let file_inline_suppressed = cursor.file_ignore_tags.contains("inline");
                    if !file_inline_suppressed && !per_decl_suppressed {
                        // Find inline metadata from file_symbols.
                        let func_sym = cursor
                            .file_symbols
                            .functions
                            .iter()
                            .find(|f| f.name == group.name);
                        let inline_expr = func_sym
                            .and_then(|f| f.inline_return_text.clone())
                            .unwrap_or_default();
                        let inline_is_compound =
                            func_sym.map(|f| f.inline_is_compound).unwrap_or(false);

                        // Find the full function range from the AST.
                        let func_range = ast.items.iter().find_map(|item| {
                            if let Statement::Function(f) = item {
                                let fname = f.name.as_ref().map(|id| {
                                    rope.slice_to_cow(id.node.start_byte()..id.node.end_byte())
                                        .to_string()
                                });
                                if fname.as_deref() == Some(&group.name) {
                                    Some(f.node.to_range(rope))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        });

                        let data = serde_json::json!({
                            "inline_name": group.name,
                            "inline_expr": inline_expr,
                            "inline_is_compound": inline_is_compound,
                            "inline_func_range": func_range,
                        });

                        all_diagnostics.push(Diagnostic {
                            range: decl_occ.range.clone(),
                            message: crate::util::i18n::inlinable_function(&group.name),
                            severity: Some(DiagnosticSeverity::Hint),
                            tags: Some(vec![
                                crate::lsp::diagnostic::lsp::DiagnosticTag::Unnecessary,
                            ]),
                            source: Some("jass".into()),
                            code: Some(crate::lsp::diagnostic::lsp::DiagnosticCode::String("inline".into())),
                            data: Some(data),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }

    // 8b. Diagnostic: `//set build-*` in a non-entry file.
    //     Build directives must always be in `//entry` files regardless of
    //     whether any entry points exist in the component.
    if !new_snapshot_is_entry {
        for (key, _) in &cursor_file_settings {
            if key == "build-jass" || key == "build-as" || key == "backup" || key == "build-uglify" || key == "build-before" || key == "build-after" {
                // Find the SetDir node in the AST to get its range.
                for item in &ast.items {
                    if let Statement::SetDir(sd) = item {
                        if sd.key == *key {
                            all_diagnostics.push(Diagnostic {
                                range: sd.node.to_range(rope),
                                message: crate::util::i18n::build_requires_entry(key),
                                severity: Some(DiagnosticSeverity::Error),
                                ..Diagnostic::new("jass", "build-not-entry")
                            });
                        }
                    }
                }
            }
        }
    }

    // ── FINAL cancellation check — don't store stale results ──
    if cancel.is_cancelled() {
        // Restore the pre-preliminary snapshot so the SemanticTokens handler
        // never reads an empty placeholder.
        match true_old_snapshot {
            Some(snap) => {
                FILE_STORE.insert(uri.clone(), snap);
            }
            None => {
                FILE_STORE.remove(uri);
            }
        }
        return Ok(vec![]);
    }

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
        _type_map: cursor.type_map,
        type_hints: cursor.type_hints,
        ujapi_hints,
        func_decl_keys,
        colors: cursor.colors,
    });

    // Persist to unified disk cache.
    if let Some(meta) = file_cache::FileMeta::from_uri(uri) {
        file_cache::store(
            uri,
            meta,
            hash,
            &new_snapshot.file_symbols,
            &new_snapshot.ref_map,
            &new_snapshot.func_decl_keys,
        );
    }

    // ── Atomic store — single source of truth ──
    FILE_STORE.insert(uri.clone(), new_snapshot.clone());

    // ── Recompute entry-point cache after FILE_STORE update ──
    IMPORT_GRAPH.recompute_entry_cache();

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
/// store it in [`FILE_STORE`].  Diagnostics are delivered via the pull model
/// when the client re-requests them after `workspace/diagnostics/refresh`.
///
/// **Fast path:** if the file's metadata (size + mtime) hasn't changed since
/// the last successful parse AND we already have a snapshot in [`FILE_STORE`],
/// the function returns immediately — no `read_to_string`, no tree-sitter
/// parse, no cursor walk.
///
/// Returns the cascade list (peer URIs whose exports may have changed).
pub async fn parse_from_disk(uri: &Url) -> Result<Vec<Url>, Box<dyn Error + Send + Sync>> {
    let path = uri.to_file_path().map_err(|_| "invalid file path")?;
    if !path.exists() {
        return Ok(vec![]);
    }

    // Cheap stat()-based skip: if file unchanged and we have results, skip.
    if let Some(current_meta) = file_cache::FileMeta::from_path(&path) {
        if let Some(cached) = file_cache::load(uri) {
            if cached.meta == current_meta && FILE_STORE.contains_key(uri) {
                return Ok(vec![]);
            }
        }
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

    tokio::select! {
        res = handle => res.map_err(|e| -> Box<dyn Error + Send + Sync> { e.into() })?,
        _ = token.cancelled() => Ok(vec![]),
    }
}
