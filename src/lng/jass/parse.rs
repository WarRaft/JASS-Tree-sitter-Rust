use crate::lng::jass::ast::{build_ast, rewrite_imports, Statement};
use crate::lng::jass::cursor::Cursor;
use crate::lng::jass::uri_map::TREE_MAP;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity, DocumentDiagnosticReport};
use crate::lsp::diagnostic::uri_map::URI_MAP as DIAGNOSTIC_URI_MAP;
use crate::lsp::document_link::lsp::DocumentLink;
use crate::lsp::document_link::uri_map::URI_MAP as LINK_URI_MAP;
use crate::lsp::document_symbol::uri_map::URI_MAP as SYMBOL_URI_MAP;
use crate::lsp::folding::uri_map::URI_MAP as FOLDING_URI_MAP;
use crate::lsp::position::Position;
use crate::lsp::range::Range;
use crate::lsp::semantic::uri_map::URI_MAP as SEMANTIC_URI_MAP;
use crate::util::import_graph::{resolve_import, IMPORT_GRAPH};
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_lock::uri_unlock;
use std::collections::HashSet;
use std::error::Error;
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

    // 2. Rewrite leading `//import` / `//import!` comments into Import nodes
    let src: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
    rewrite_imports(&mut ast, &src);

    // 3. Extract resolved import URLs, document links, and diagnostics
    //    for non-existent import paths.
    let mut imports = HashSet::new();
    let mut links = Vec::new();
    let mut import_diagnostics = Vec::new();

    for item in &ast.items {
        if let Statement::Import(imp) = item {
            if imp.path.is_empty() {
                continue; // cursor already emits "Missing import path"
            }

            // Compute the range of the path portion within the import node.
            let node = &imp.node;
            let prefix_len = if imp.frozen {
                "//import!".len()
            } else {
                "//import".len()
            };
            let node_text = std::str::from_utf8(
                &src[node.start_byte()..node.end_byte()],
            )
            .unwrap_or("");
            let after_prefix = &node_text[prefix_len..];
            let ws_len = after_prefix.len() - after_prefix.trim_start().len();
            let path_start_byte = node.start_byte() + prefix_len + ws_len;
            let path_end_byte = node.start_byte() + prefix_len + ws_len + imp.path.len();

            let path_range = Range {
                start: Position::from_byte_offset(rope, path_start_byte)
                    .unwrap_or_default(),
                end: Position::from_byte_offset(rope, path_end_byte)
                    .unwrap_or_default(),
            };

            match resolve_import(uri, &imp.path) {
                Some(resolved) => {
                    imports.insert(resolved.url.clone());

                    if resolved.exists {
                        // Clickable link → opens the file
                        links.push(DocumentLink {
                            range: path_range,
                            target: Some(resolved.url.to_string()),
                            tooltip: Some(resolved.url.to_string()),
                        });
                    } else {
                        // File does not exist → diagnostic error
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

    // 4. Single-pass cursor: diagnostics + symbols + folding + id_roles + scopes
    let cursor = Cursor::walk(&ast, rope);

    // 5. Merge import diagnostics with cursor diagnostics
    let mut all_diagnostics = cursor.diagnostics;
    all_diagnostics.extend(import_diagnostics);

    // 6. Store results
    let report = DocumentDiagnosticReport::Full {
        result_id: None,
        items: all_diagnostics,
        related_documents: None,
    };

    FOLDING_URI_MAP.insert(uri.clone(), cursor.folding);
    SYMBOL_URI_MAP.insert(uri.clone(), cursor.symbols);
    DIAGNOSTIC_URI_MAP.insert(uri.clone(), report);
    SEMANTIC_URI_MAP.insert(uri.clone(), cursor.semantic);
    LINK_URI_MAP.insert(uri.clone(), links);

    Ok(())
}
