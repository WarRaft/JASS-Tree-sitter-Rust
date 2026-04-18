use crate::http::document_link::DocumentLink;
use crate::http::inlay_hint::InlayHint;
use crate::lng::jass::ast::{Ast, Statement, annotate_comptime_values, build_ast, rewrite_imports};
use crate::lng::jass::cursor::{Cursor, ImportedSymbol};
use crate::http::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::http::ref_map::{DeclKey, RefMap, build_ref_map};
use crate::util::file_cache;
use crate::util::parse_cache::{
    PARSE_CACHE, ParseSnapshot, exports_changed, new_cancel_token,
};
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::parse::{
    ParseFn, cascade_parse_and_notify, ensure_visible_component_loaded,
    resolve_import_directive, all_visible_entries,
};
use crate::util::roper::node::NodeExt;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::tree_map::TREE_MAP;
use crate::util::builder_process;
use lapce_xi_rope::Rope;
use std::collections::HashSet;
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use url::Url;

struct JassAnalysisResult {
    previous_snapshot: Option<Arc<ParseSnapshot>>,
    new_snapshot: Arc<ParseSnapshot>,
    old_component: HashSet<Url>,
    new_component: HashSet<Url>,
    hash: [u8; 32],
}

struct JassImportResolution {
    imports: HashSet<Url>,
    frozen_imports: HashSet<Url>,
    links: Vec<DocumentLink>,
    import_diagnostics: Vec<Diagnostic>,
    ujapi_hints: Vec<InlayHint>,
}

struct JassVisibleScopePrep {
    old_component: HashSet<Url>,
    new_component: HashSet<Url>,
    imported_symbols: Vec<ImportedSymbol>,
}

struct JassPreparedAnalysis<'tree> {
    ast: Ast<'tree>,
    import_resolution: JassImportResolution,
    visible_scope: JassVisibleScopePrep,
}

// ─── Main parse entry point ─────────────────────────────────────────────────

/// Parse and store all LSP data for `uri`.
///
/// The heavy `_parse` work runs on the blocking thread pool so that tokio
/// worker threads stay free for I/O and other request handlers.
///
/// Returns the list of peer URIs that should be cascade-re-parsed.
pub async fn parse(uri: &Url) -> Result<Vec<Url>, Box<dyn Error + Send + Sync>> {
    let started_at = Instant::now();
    crate::debug_log!("jass::parse START uri={}", uri.path());

    // ── Clone owned data from DashMap guards and drop the guards immediately ──
    let rope = ROPE_MAP
        .get(uri)
        .map(|r| r.value().clone())
        .ok_or("no rope")?;
    let tree = TREE_MAP
        .get(uri)
        .map(|t| t.value().clone())
        .ok_or("no tree")?;

    let result = run_parse_task(uri, rope, tree).await;
    crate::debug_log!(
        "jass::parse END uri={}, result={}, elapsed_ms={}",
        uri.path(),
        if result.is_ok() { "OK" } else { "ERR" },
        started_at.elapsed().as_millis()
    );
    result
}

async fn run_parse_task(
    uri: &Url,
    rope: Rope,
    tree: tree_sitter::Tree,
) -> Result<Vec<Url>, Box<dyn Error + Send + Sync>> {
    let token = new_cancel_token(uri);
    let uri_owned = uri.clone();
    let cancel = token.clone();

    crate::debug_log!("jass::parse spawning blocking task uri={}", uri.path());
    let handle = tokio::task::spawn_blocking(move || {
        crate::debug_log!("jass::_parse blocking task START uri={}", uri_owned.path());
        let result = _parse(&uri_owned, &rope, &tree, &cancel);
        crate::debug_log!("jass::_parse blocking task END uri={}", uri_owned.path());
        result
    });

    crate::debug_log!("jass::parse waiting for blocking result uri={}", uri.path());
    tokio::select! {
        res = handle => res.map_err(|e| -> Box<dyn Error + Send + Sync> { e.into() })?,
        _ = token.cancelled() => {
            crate::debug_log!("jass::parse cancelled uri={}", uri.path());
            Ok(vec![])
        },
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
    let started_at = Instant::now();
    crate::debug_log!(
        "jass::parse_and_notify START uri={}, generation={}",
        uri.path(),
        generation.map(|g| g.to_string()).unwrap_or_else(|| "-".into())
    );
    let parse_fn: ParseFn = Box::new(|u| Box::pin(async move { parse(&u).await }));
    crate::debug_log!("jass::parse_and_notify calling cascade_parse_and_notify");
    let result = cascade_parse_and_notify(uri, &parse_fn, None, generation).await;
    crate::debug_log!(
        "jass::parse_and_notify END uri={}, result={}, elapsed_ms={}",
        uri.path(),
        if result.is_ok() { "OK" } else { "ERR" },
        started_at.elapsed().as_millis()
    );
    result
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
    let started_at = Instant::now();
    let Some(prepared) = _prepare_jass_analysis(uri, rope, tree, cancel)? else {
        return Ok(vec![]);
    };

    let Some(analysis) = _analyze_jass(uri, rope, prepared, cancel)? else {
        return Ok(vec![]);
    };

    let JassAnalysisResult {
        previous_snapshot,
        new_snapshot,
        old_component,
        new_component: component,
        hash,
    } = analysis;

    // Persist to unified disk cache.
    if let Some(meta) = file_cache::FileMeta::from_uri(uri) {
        file_cache::store(
            uri,
            meta,
            hash,
            &new_snapshot.file_symbols,
            &new_snapshot.ref_map,
            &new_snapshot.func_decl_keys,
            &new_snapshot.var_decl_keys,
            &new_snapshot.arg_decl_keys,
        );
    }

    // ── Atomic store — single source of truth ──
    PARSE_CACHE.insert(uri.clone(), new_snapshot.clone());

    // ── Persist entry-point status so it survives server restarts ──
    let entry_changed = IMPORT_GRAPH.mark_entry(uri, new_snapshot.file_symbols.is_entry);

    // ── Recompute entry-point cache after PARSE_CACHE update ──
    if entry_changed {
        IMPORT_GRAPH.recompute_entry_cache();
    }

    // 11. Spawn builder process for the entry point (if any).
    //
    // When a file in an import tree is parsed, we find its entry point.
    // The builder process runs in the background to collect multi-file
    // diagnostics (unused functions, type mismatches across files, etc.).
    //
    // If a new parse starts before the builder finishes, the old builder
    // is cancelled and a new one is spawned.
    if let Some(entry_uri) = crate::lng::jass::builder::collect::find_entry_point(uri) {
        builder_process::cancel_builder_for_entry(&entry_uri);

        let config = builder_process::BuilderConfig {
            mode: crate::lng::jass::builder::PipelineMode::Diagnostics,
            opts: crate::lng::jass::builder::collect::build_opt_tags(
                &new_snapshot.file_symbols.file_settings,
            ),
        };
        builder_process::spawn_builder_task(&entry_uri, config);
    }

    // 10. Export diff — decide on cascade.
    let did_change = exports_changed(previous_snapshot.as_deref(), &new_snapshot);

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

    crate::debug_log!(
        "jass::_parse internal END uri={}, diagnostics={}, cascade_count={}, exports_changed={}, total_elapsed_ms={}",
        uri.path(),
        new_snapshot.diagnostics.len(),
        cascade.len(),
        did_change,
        started_at.elapsed().as_millis()
    );
    Ok(cascade)
}

fn collect_jass_import_resolution(
    uri: &Url,
    ast: &Ast<'_>,
    src: &[u8],
    rope: &Rope,
) -> JassImportResolution {
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

    JassImportResolution {
        imports,
        frozen_imports,
        links,
        import_diagnostics,
        ujapi_hints,
    }
}

fn build_jass_imported_symbols(uri: &Url, component: &HashSet<Url>) -> Vec<ImportedSymbol> {
    // O(1)-per-name: get all symbols from the connected component.
    let visible_entries = all_visible_entries(component);
    crate::debug_log!(
        "jass::_parse visible entries uri={}, component_size={}, visible_entries={}",
        uri.path(),
        component.len(),
        visible_entries.len()
    );

    let imported_symbols = crate::util::parse::jass_imported_symbols_from_entries(
        uri,
        &visible_entries,
        true,
    );

    crate::debug_log!(
        "jass::_parse imported symbols built uri={}, imported_symbols={}",
        uri.path(),
        imported_symbols.len()
    );

    imported_symbols
}

fn collect_jass_call_graph_diagnostics(
    uri: &Url,
    file_symbols: crate::lng::symbol::FileSymbols,
    func_decl_keys: HashSet<DeclKey>,
    var_decl_keys: HashSet<DeclKey>,
    arg_decl_keys: HashSet<DeclKey>,
) -> (
    Option<Arc<ParseSnapshot>>,
    crate::util::call_graph::FuncDiagnostics,
) {
    let previous_snapshot = PARSE_CACHE.get(uri).map(|e| Arc::clone(e.value()));

    let preliminary = Arc::new(ParseSnapshot {
        folding: Vec::new(),
        symbols: Vec::new(),
        semantic: std::sync::RwLock::new(Default::default()),
        diagnostics: Vec::new(),
        links: Vec::new(),
        ref_map: RefMap::default(),
        file_symbols,
        _type_map: Default::default(),
        type_hints: Vec::new(),
        ujapi_hints: Vec::new(),
        func_decl_keys,
        var_decl_keys,
        arg_decl_keys,
        colors: Vec::new(),
    });
    PARSE_CACHE.insert(uri.clone(), preliminary);

    let diagnostics = crate::util::call_graph::diagnose_functions(uri);

    match previous_snapshot.as_ref() {
        Some(snap) => {
            PARSE_CACHE.insert(uri.clone(), Arc::clone(snap));
        }
        None => {
            PARSE_CACHE.remove(uri);
        }
    }

    (previous_snapshot, diagnostics)
}

fn prepare_jass_visible_scope(
    uri: &Url,
    imports: &HashSet<Url>,
    frozen_imports: &HashSet<Url>,
) -> JassVisibleScopePrep {
    // Capture the old visible component BEFORE updating the graph,
    // so we can detect peers that lost visibility after import removal.
    let old_component = IMPORT_GRAPH.visible_component(uri);

    // Register frozen targets eagerly — before visible_component — so
    // tree_for_uri can prune incoming edges even when PARSE_CACHE doesn't
    // have this file's snapshot yet (first parse / cold start).
    let _ = IMPORT_GRAPH.mark_frozen(frozen_imports);

    let graph_changed = IMPORT_GRAPH.update(uri, imports.clone());

    // Refresh entry cache so that visible_component reads the updated graph.
    if graph_changed {
        IMPORT_GRAPH.recompute_entry_cache();
    }

    let mut component = IMPORT_GRAPH.visible_component(uri);
    crate::debug_log!(
        "jass::_parse visible component initial uri={}, component_size={}",
        uri.path(),
        component.len()
    );

    component = ensure_visible_component_loaded(uri, component, Some(uri));

    let imported_symbols = build_jass_imported_symbols(uri, &component);

    JassVisibleScopePrep {
        old_component,
        new_component: component,
        imported_symbols,
    }
}

fn _prepare_jass_analysis<'tree>(
    uri: &Url,
    rope: &Rope,
    tree: &'tree tree_sitter::Tree,
    cancel: &CancellationToken,
) -> Result<Option<JassPreparedAnalysis<'tree>>, Box<dyn Error + Send + Sync>> {
    let started_at = Instant::now();
    crate::debug_log!("jass::_parse internal START uri={}", uri.path());
    let root = tree.root_node();

    crate::debug_log!("jass::_parse building AST uri={}", uri.path());
    let mut ast = build_ast(root);
    crate::debug_log!(
        "jass::_parse AST built uri={}, item_count={}, elapsed_ms={}",
        uri.path(),
        ast.items.len(),
        started_at.elapsed().as_millis()
    );

    if cancel.is_cancelled() {
        crate::debug_log!("jass::_parse cancelled at checkpoint 1");
        return Ok(None);
    }

    crate::debug_log!("jass::_parse rewriting imports uri={}", uri.path());
    let src: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
    rewrite_imports(&mut ast, &src);
    annotate_comptime_values(&mut ast, &src);

    crate::debug_log!("jass::_parse resolving imports uri={}", uri.path());
    let import_resolution = collect_jass_import_resolution(uri, &ast, &src, rope);
    crate::debug_log!(
        "jass::_parse imports resolved uri={}, imports={}, frozen_imports={}, links={}, import_diagnostics={}, ujapi_hints={}, elapsed_ms={}",
        uri.path(),
        import_resolution.imports.len(),
        import_resolution.frozen_imports.len(),
        import_resolution.links.len(),
        import_resolution.import_diagnostics.len(),
        import_resolution.ujapi_hints.len(),
        started_at.elapsed().as_millis()
    );

    if cancel.is_cancelled() {
        return Ok(None);
    }

    let visible_scope = prepare_jass_visible_scope(
        uri,
        &import_resolution.imports,
        &import_resolution.frozen_imports,
    );

    if cancel.is_cancelled() {
        return Ok(None);
    }

    Ok(Some(JassPreparedAnalysis {
        ast,
        import_resolution,
        visible_scope,
    }))
}

fn _analyze_jass(
    uri: &Url,
    rope: &Rope,
    prepared: JassPreparedAnalysis<'_>,
    cancel: &CancellationToken,
) -> Result<Option<JassAnalysisResult>, Box<dyn Error + Send + Sync>> {
    let started_at = Instant::now();
    let JassPreparedAnalysis {
        ast,
        import_resolution,
        visible_scope,
    } = prepared;
    let JassImportResolution {
        frozen_imports,
        links,
        import_diagnostics,
        ujapi_hints,
        ..
    } = import_resolution;
    let JassVisibleScopePrep {
        old_component,
        new_component: component,
        imported_symbols,
    } = visible_scope;

    // ── Cancellation checkpoint ──
    if cancel.is_cancelled() {
        return Ok(None);
    }

    // 5. Single-pass cursor: diagnostics + symbols + folding + id_roles + scopes
    let mut cursor = Cursor::walk(&ast, rope, &imported_symbols);
    crate::debug_log!(
        "jass::_parse cursor walk done uri={}, diagnostics={}, functions={}, globals={}, types={}, elapsed_ms={}",
        uri.path(),
        cursor.diagnostics.len(),
        cursor.file_symbols.functions.len(),
        cursor.file_symbols.globals.len(),
        cursor.file_symbols.types.len(),
        started_at.elapsed().as_millis()
    );
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
    let var_decl_keys = cursor.var_decl_keys;
    let arg_decl_keys = cursor.arg_decl_keys;
    let ref_map = build_ref_map(
        cursor.ref_groups,
        cursor.ref_names,
        cursor.external_decls,
        rope,
    );
    let hash = file_cache::content_hash(rope);

    // 8. Call-graph diagnostics: unused functions & cyclic calls.
    //    `diagnose_functions` still reads the current file via `PARSE_CACHE`,
    //    so a small compatibility bridge publishes a temporary preliminary
    //    snapshot, runs the analysis, and restores the previous cache state.
    let (true_old_snapshot, func_diag) = collect_jass_call_graph_diagnostics(
        uri,
        cursor.file_symbols.clone(),
        func_decl_keys.clone(),
        var_decl_keys.clone(),
        arg_decl_keys.clone(),
    );
    {
        crate::debug_log!(
            "jass::_parse call graph diagnostics uri={}, unused={}, in_cycle={}, inlinable={}",
            uri.path(),
            func_diag.unused.len(),
            func_diag.in_cycle.len(),
            func_diag.inlinable.len()
        );

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
                                crate::http::diagnostic::DiagnosticTag::Unnecessary,
                            ]),
                            source: Some("jass".into()),
                            code: Some(crate::http::diagnostic::DiagnosticCode::String("unused-function".into())),
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
                                crate::http::diagnostic::DiagnosticTag::Unnecessary,
                            ]),
                            source: Some("jass".into()),
                            code: Some(crate::http::diagnostic::DiagnosticCode::String("inline".into())),
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
            if key == "build-jass" || key == "build-as" || key == "backup" || key == "build-opts" || key == "build-uglify" || key == "build-before" || key == "build-after" {
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
        return Ok(None);
    }

    // 9. Build snapshot candidate.
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
        var_decl_keys,
        arg_decl_keys,
        colors: cursor.colors,
    });

    crate::debug_log!(
        "jass::_parse analysis END uri={}, diagnostics={}, old_component_size={}, new_component_size={}, total_elapsed_ms={}",
        uri.path(),
        new_snapshot.diagnostics.len(),
        old_component.len(),
        component.len(),
        started_at.elapsed().as_millis()
    );

    Ok(Some(JassAnalysisResult {
        previous_snapshot: true_old_snapshot,
        new_snapshot,
        old_component,
        new_component: component,
        hash,
    }))
}

/// Parse a **closed** file from disk, produce a full [`ParseSnapshot`], and
/// store it in [`PARSE_CACHE`].  Diagnostics are delivered via the pull model
/// when the client re-requests them after `workspace/diagnostics/refresh`.
///
/// **Fast path:** if the file's metadata (size + mtime) hasn't changed since
/// the last successful parse AND we already have a snapshot in [`PARSE_CACHE`],
/// the function returns immediately — no `read_to_string`, no tree-sitter
/// parse, no cursor walk.
///
/// Returns the cascade list (peer URIs whose exports may have changed).
pub async fn parse_from_disk(uri: &Url) -> Result<Vec<Url>, Box<dyn Error + Send + Sync>> {
    let started_at = Instant::now();
    crate::debug_log!("jass::parse_from_disk START uri={}", uri.path());
    let path = uri.to_file_path().map_err(|_| "invalid file path")?;
    if !path.exists() {
        return Ok(vec![]);
    }

    // Cheap stat()-based skip: if the disk cache is fresh, no re-parse needed.
    // Closed files are intentionally absent from PARSE_CACHE (peek_or_load
    // handles lazy loading), so we must NOT require PARSE_CACHE presence here.
    if let Some(cached) = file_cache::load_if_fresh(uri) {
        crate::debug_log!("jass::parse_from_disk SKIP (disk_cache fresh) uri={}", uri.path());
        let _ = cached; // symbols already in disk cache; peek_or_load serves them
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&path)?;
    let rope = Rope::from(content.as_str());

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_jass::language().into())?;
    let tree = parser.parse(&content, None).ok_or("parse failed")?;

    let result = run_parse_task(uri, rope, tree).await;
    crate::debug_log!(
        "jass::parse_from_disk END uri={}, result={}, elapsed_ms={}",
        uri.path(),
        if result.is_ok() { "OK" } else { "ERR" },
        started_at.elapsed().as_millis()
    );
    result
}
