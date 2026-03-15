use crate::lng::ass::ast::{build_ast, rewrite_directives, TopLevel};
use crate::lng::ass::cursor::Cursor;
use crate::lng::ass::uri_map::TREE_MAP;
use crate::lng::jass::symbol::FileSymbols;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity};
use crate::lsp::document_link::lsp::DocumentLink;
use crate::lsp::position::Position;
use crate::lsp::range::Range;
use crate::lsp::ref_map::RefMap;
use crate::util::file_store::{ParseSnapshot, FILE_STORE};
use crate::util::import_graph::{resolve_import, IMPORT_GRAPH};
use crate::util::roper::node::NodeExt;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_lock::uri_unlock;
use std::collections::HashSet;
use std::error::Error;
use std::sync::Arc;
use url::Url;

// ─── Main parse entry point ─────────────────────────────────────────────────

pub async fn parse(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    let result = _parse(uri);
    uri_unlock(uri);
    result
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

                match resolve_import(uri, &path_text) {
                    Some(resolved) => {
                        imports.insert(resolved.url.clone());

                        if resolved.exists {
                            links.push(DocumentLink {
                                range: path_range,
                                target: Some(resolved.url.to_string()),
                                tooltip: Some(resolved.url.to_string()),
                            });
                        } else {
                            import_diagnostics.push(Diagnostic {
                                range: path_range,
                                message: format!("File not found: {}", path_text),
                                severity: Some(DiagnosticSeverity::Error),
                                ..Default::default()
                            });
                        }
                    }
                    None => {
                        import_diagnostics.push(Diagnostic {
                            range: path_range,
                            message: format!("Cannot resolve import path: {}", path_text),
                            severity: Some(DiagnosticSeverity::Error),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        // //import / //import! directives
        if let TopLevel::ImportDir(imp) = item {
            if imp.path.is_empty() {
                continue; // cursor already emits "Missing import path"
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
                        import_diagnostics.push(Diagnostic {
                            range: path_range,
                            message: format!("File not found: {}", imp.path),
                            severity: Some(DiagnosticSeverity::Error),
                            ..Default::default()
                        });
                    }
                }
                None => {
                    import_diagnostics.push(Diagnostic {
                        range: path_range,
                        message: format!("Cannot resolve import path: {}", imp.path),
                        severity: Some(DiagnosticSeverity::Error),
                        ..Default::default()
                    });
                }
            }
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
        func_decl_keys: HashSet::new(),
    });

    FILE_STORE.insert(uri.clone(), snapshot);

    Ok(())
}
