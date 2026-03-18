use crate::lng::ass::ast::{build_ast, rewrite_directives, TopLevel};
use crate::lng::ass::cursor::Cursor;
use crate::lng::jass::symbol::FileSymbols;
use crate::lng::jass::type_map::TypeMap;
use crate::lsp::ref_map::RefMap;
use crate::util::file_store::{
    exports_changed, new_cancel_token, ParseSnapshot, FILE_STORE,
};
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::parse::{cascade_parse_and_notify, resolve_import_directive, resolve_path_import, ParseFn};
use crate::util::roper::node::NodeExt;
use crate::util::roper::uri_map::ROPE_MAP;
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
/// Intended to be called from a **spawned task** (not the main message loop).
pub async fn parse_and_notify(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parse_fn: ParseFn = Box::new(|u| {
        Box::pin(async move { parse(&u).await })
    });
    cascade_parse_and_notify(uri, &parse_fn, None).await
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

    // 4. Single-pass cursor: diagnostics + symbols + folding + id_roles
    let cursor = Cursor::walk(&ast, rope);

    // 5. Merge import diagnostics with cursor diagnostics
    let mut all_diagnostics = cursor.diagnostics;
    all_diagnostics.extend(import_diagnostics);

    // ── FINAL cancellation check — don't store stale results ──
    if cancel.is_cancelled() { return Ok(vec![]); }

    // 6. Build snapshot and store atomically in FILE_STORE.
    let mut file_symbols = FileSymbols::new();
    file_symbols.frozen_imports = frozen_imports;
    file_symbols.file_settings = cursor.file_settings;

    let old_snapshot = FILE_STORE.get(uri).map(|e| Arc::clone(e.value()));

    let new_snapshot = Arc::new(ParseSnapshot {
        folding: cursor.folding,
        symbols: cursor.symbols,
        semantic: std::sync::RwLock::new(cursor.semantic),
        diagnostics: all_diagnostics,
        links,
        ref_map: RefMap::default(),
        file_symbols,
        _type_map: TypeMap::default(),
        type_hints: vec![],
        ujapi_hints,
        func_decl_keys: HashSet::new(),
    });

    FILE_STORE.insert(uri.clone(), new_snapshot.clone());

    // 7. Export diff — decide on cascade.
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
