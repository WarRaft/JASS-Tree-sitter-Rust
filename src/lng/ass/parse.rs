use crate::lng::ass::ast::{build_ast, rewrite_directives, TopLevel};
use crate::lng::ass::cursor::{Cursor, ImportedKind, ImportedSymbol};
use crate::lng::jass::symbol::FileSymbols;
use crate::lng::jass::type_map::TypeMap;
use crate::lsp::ref_map::{build_ref_map, DeclKey};
use crate::util::file_cache;
use crate::util::file_store::{
    exports_changed, new_cancel_token, register_pending, ParseSnapshot, FILE_STORE,
};
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::parse::{
    as_file_symbols_to_entries, cascade_parse_and_notify, ensure_file_symbols,
    find_decl_key_by_name, resolve_import_directive, resolve_path_import, ParseFn,
};
use crate::util::roper::node::NodeExt;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::scope_resolver::{SymbolNS, SCOPE_RESOLVER};
use crate::util::tree_map::TREE_MAP;
use lapce_xi_rope::Rope;
use std::collections::HashSet;
use std::error::Error;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use url::Url;

// ─── Jass namespace constant ────────────────────────────────────────────────

/// Namespace assigned to entities imported from `.j` (JASS) files.
const JASS_NAMESPACE: &str = "Jass";

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
/// Intended to be called from a **spawned task** (not the main message loop).
pub async fn parse_and_notify(uri: &Url, generation: Option<u64>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parse_fn: ParseFn = Box::new(|u| {
        Box::pin(async move { parse(&u).await })
    });
    cascade_parse_and_notify(uri, &parse_fn, None, generation).await
}

/// Core parse logic (runs on the blocking thread pool).
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

    // 2. Rewrite leading `//import` / `//import!` / `//set` comments into directive nodes
    let src: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
    rewrite_directives(&mut ast, &src);

    // 3. Extract imports from BOTH #include directives AND //import directives
    let mut imports = HashSet::new();
    let mut frozen_imports = HashSet::new();
    let mut links = Vec::new();
    let mut import_diagnostics = Vec::new();
    let mut ujapi_hints = Vec::new();

    for item in &ast.items {
        // Native #include
        if let TopLevel::Include(incl) = item {
            if let Some(path_node) = &incl.path {
                let path_text = path_node.text(rope);
                let path_range = path_node.to_range(rope);

                resolve_path_import(
                    uri, &path_text, path_range,
                    &mut imports, &mut links, &mut import_diagnostics,
                );
            }
        }

        // //import / //import! directives
        if let TopLevel::ImportDir(imp) = item {
            resolve_import_directive(
                uri, imp, &src, rope,
                &mut imports, &mut frozen_imports, &mut links, &mut import_diagnostics,
            );
        }

        // //import-ujapi! directives
        if let TopLevel::UjapiDir(ud) = item {
            crate::util::parse::resolve_ujapi_directive(
                uri, ud, &src, rope,
                &mut imports, &mut frozen_imports, &mut links, &mut import_diagnostics,
                &mut ujapi_hints,
            );
        }
    }

    // ── Cancellation checkpoint ──
    if cancel.is_cancelled() { return Ok(vec![]); }

    // Capture the old connected component BEFORE updating the graph.
    let old_component = IMPORT_GRAPH.connected_component(uri);

    IMPORT_GRAPH.update(uri, imports);

    let component = IMPORT_GRAPH.connected_component(uri);

    // ── Cancellation checkpoint ──
    if cancel.is_cancelled() { return Ok(vec![]); }

    // 4. Gather symbols from the **entire connected component** (unified scope).
    //    Symbols from `.j` files are placed under the `Jass` namespace;
    //    symbols from `.as` files keep their original namespace.
    //    JASS types are also promoted to top-level (namespace = "") so that
    //    AS code can reference them without qualifier (e.g. `unit`, `handle`).
    let mut imported_symbols: Vec<ImportedSymbol> = Vec::new();
    {
        // Ensure every peer is in the scope resolver.
        for peer_uri in &component {
            if peer_uri != uri {
                let ts_lang = if crate::util::open::is_as_uri(peer_uri) {
                    tree_sitter_as::language().into()
                } else {
                    tree_sitter_jass::language().into()
                };
                if !ensure_file_symbols(peer_uri, ts_lang) {
                    register_pending(peer_uri, uri);
                    log::info!(
                        "pending import: {} waits for {}",
                        uri.path(),
                        peer_uri.path()
                    );
                }
            }
        }

        let visible_entries = SCOPE_RESOLVER.all_visible(&component);

        for entry in &visible_entries {
            // Skip entries from the file being parsed — cursor builds them locally.
            if &entry.uri == uri {
                continue;
            }

            let is_jass_file = !crate::util::open::is_as_uri(&entry.uri);

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

            let sym_kind = match entry.ns {
                SymbolNS::Func => ImportedKind::Func,
                SymbolNS::Var => ImportedKind::Var,
            };

            if is_jass_file {
                // JASS entities → placed under the `Jass` namespace.
                imported_symbols.push(ImportedSymbol {
                    origin_uri: entry.uri.clone(),
                    name: entry.name.clone(),
                    kind: sym_kind,
                    origin_decl_key,
                    return_type: entry.return_type.clone(),
                    type_name: entry.type_name.clone(),
                    namespace: JASS_NAMESPACE.to_string(),
                });

                // JASS types are also available unqualified (bare name)
                // so that `class MyUnit : unit` works without `Jass::`.
                // JASS variables and functions are also promoted for
                // compatibility since they share the same names.
                imported_symbols.push(ImportedSymbol {
                    origin_uri: entry.uri.clone(),
                    name: entry.name.clone(),
                    kind: sym_kind,
                    origin_decl_key,
                    return_type: entry.return_type.clone(),
                    type_name: entry.type_name.clone(),
                    namespace: String::new(),
                });
            } else {
                // AS entities → keep their original namespace.
                imported_symbols.push(ImportedSymbol {
                    origin_uri: entry.uri.clone(),
                    name: entry.name.clone(),
                    kind: sym_kind,
                    origin_decl_key,
                    return_type: entry.return_type.clone(),
                    type_name: entry.type_name.clone(),
                    namespace: entry.namespace.clone(),
                });
            }
        }
    }

    // When any `.j` file is connected, the engine implicitly provides
    // the `handle` base type.  Inject it if not already imported.
    let has_jass = component.iter().any(|u| u != uri && !crate::util::open::is_as_uri(u));
    if has_jass {
        let already_has_handle = imported_symbols.iter()
            .any(|s| s.name == "handle" && s.kind == ImportedKind::Var);
        if !already_has_handle {
            if let Some(jass_uri) = component.iter().find(|u| !crate::util::open::is_as_uri(u)) {
                imported_symbols.push(ImportedSymbol {
                    origin_uri: jass_uri.clone(),
                    name: "handle".to_string(),
                    kind: ImportedKind::Var,
                    origin_decl_key: None,
                    return_type: None,
                    type_name: None,
                    namespace: String::new(),
                });
            }
        }
    }

    // ── Cancellation checkpoint ──
    if cancel.is_cancelled() { return Ok(vec![]); }

    // 5. Two-phase cursor: diagnostics + symbols + folding + id_roles + scopes + ref linking
    let cursor = Cursor::walk(&ast, rope, &imported_symbols);

    // 6. Merge import diagnostics with cursor diagnostics
    let mut all_diagnostics = cursor.diagnostics;
    all_diagnostics.extend(import_diagnostics);

    // ── FINAL cancellation check — don't store stale results ──
    if cancel.is_cancelled() { return Ok(vec![]); }

    // 7. Build ref_map
    let func_decl_keys = cursor.func_decl_keys;
    let ref_map = build_ref_map(cursor.ref_groups, cursor.ref_names, cursor.external_decls, rope);

    // 8. Build file_symbols for export diff and scope resolver
    let mut as_file_symbols = cursor.file_symbols;
    as_file_symbols.frozen_imports = frozen_imports;
    as_file_symbols.file_settings = cursor.file_settings;
    as_file_symbols.file_ignore_tags = cursor.file_ignore_tags;

    // Convert to JASS FileSymbols for ParseSnapshot compatibility
    let mut file_symbols = FileSymbols::new();
    file_symbols.frozen_imports = as_file_symbols.frozen_imports.clone();
    file_symbols.file_settings = as_file_symbols.file_settings.clone();
    file_symbols.file_ignore_tags = as_file_symbols.file_ignore_tags.clone();
    file_symbols.is_entry = as_file_symbols.is_entry;

    let old_snapshot = FILE_STORE.get(uri).map(|e| Arc::clone(e.value()));

    let new_snapshot = Arc::new(ParseSnapshot {
        folding: cursor.folding,
        symbols: cursor.symbols,
        semantic: std::sync::RwLock::new(cursor.semantic),
        diagnostics: all_diagnostics,
        links,
        ref_map,
        file_symbols,
        _type_map: TypeMap::default(),
        type_hints: vec![],
        ujapi_hints,
        func_decl_keys,
        colors: cursor.colors,
    });

    FILE_STORE.insert(uri.clone(), new_snapshot.clone());

    // ── Recompute entry-point cache after FILE_STORE update ──
    IMPORT_GRAPH.recompute_entry_cache();

    // 9. Update scope resolver with AS symbols
    let hash = file_cache::content_hash(rope);
    let entries = as_file_symbols_to_entries(uri, &as_file_symbols);
    SCOPE_RESOLVER.update_file(uri, hash, entries);

    // 10. Export diff — decide on cascade.
    let did_change = exports_changed(old_snapshot.as_deref(), &new_snapshot);

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
/// store it in [`FILE_STORE`].
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
    parser.set_language(&tree_sitter_as::language().into())?;
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
