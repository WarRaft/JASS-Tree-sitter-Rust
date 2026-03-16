use crate::lng::ass::ast::{build_ast, rewrite_directives, TopLevel};
use crate::lng::ass::cursor::Cursor;
use crate::lng::jass::symbol::FileSymbols;
use crate::lng::jass::type_map::TypeMap;
use crate::lsp::ref_map::RefMap;
use crate::util::file_store::{publish_diagnostics, send_refresh_all, ParseSnapshot, FILE_STORE};
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::parse::{resolve_import_directive, resolve_path_import};
use crate::util::roper::node::NodeExt;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::tree_map::TREE_MAP;
use std::collections::HashSet;
use std::error::Error;
use std::sync::Arc;
use url::Url;

// ─── Main parse entry point ─────────────────────────────────────────────────

pub async fn parse(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    _parse(uri)
}

/// Parse + push diagnostics + refresh all open editors.
///
/// Intended to be called from a **spawned task** (not the main message loop).
pub async fn parse_and_notify(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    parse(uri).await?;
    publish_diagnostics(uri).await;
    send_refresh_all().await;
    Ok(())
}

fn _parse(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    let rope_entry = ROPE_MAP.get(&uri.clone()).ok_or("no rope")?;
    let rope = rope_entry.value();

    let tree_entry = TREE_MAP.get(&uri.clone()).ok_or("no tree")?;
    let root = tree_entry.value().root_node();

    // 1. Build AST from CST
    let mut ast = build_ast(root);

    // 2. Rewrite leading `//import` / `//import!` / `//set` comments into directive nodes
    let src: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
    rewrite_directives(&mut ast, &src);

    // 3. Extract imports from BOTH #include directives AND //import directives
    let mut imports = HashSet::new();
    let mut frozen_imports = HashSet::new();
    let mut links = Vec::new();
    let mut import_diagnostics = Vec::new();

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
    }
    IMPORT_GRAPH.update(uri, imports);

    // 4. Single-pass cursor: diagnostics + symbols + folding + id_roles
    let cursor = Cursor::walk(&ast, rope);

    // 5. Merge import diagnostics with cursor diagnostics
    let mut all_diagnostics = cursor.diagnostics;
    all_diagnostics.extend(import_diagnostics);

    // 6. Build snapshot and store atomically in FILE_STORE.
    let mut file_symbols = FileSymbols::new();
    file_symbols.frozen_imports = frozen_imports;
    file_symbols.file_settings = cursor.file_settings;

    let snapshot = Arc::new(ParseSnapshot {
        folding: cursor.folding,
        symbols: cursor.symbols,
        semantic: std::sync::RwLock::new(cursor.semantic),
        diagnostics: all_diagnostics,
        links,
        ref_map: RefMap::default(),
        file_symbols,
        type_map: TypeMap::default(),
        type_hints: vec![],
        func_decl_keys: HashSet::new(),
    });

    FILE_STORE.insert(uri.clone(), snapshot);

    Ok(())
}
