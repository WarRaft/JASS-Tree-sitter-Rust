use crate::lng::ass::ast::{build_ast, TopLevel};
use crate::lng::ass::cursor::Cursor;
use crate::lng::ass::uri_map::TREE_MAP;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity, DocumentDiagnosticReport};
use crate::lsp::diagnostic::uri_map::URI_MAP as DIAGNOSTIC_URI_MAP;
use crate::lsp::document_link::lsp::DocumentLink;
use crate::lsp::document_link::uri_map::URI_MAP as LINK_URI_MAP;
use crate::lsp::document_symbol::uri_map::URI_MAP as SYMBOL_URI_MAP;
use crate::lsp::folding::uri_map::URI_MAP as FOLDING_URI_MAP;
use crate::lsp::semantic::uri_map::URI_MAP as SEMANTIC_URI_MAP;
use crate::util::import_graph::{resolve_import, IMPORT_GRAPH};
use crate::util::roper::node::NodeExt;
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
    let ast = build_ast(root);

    // 2. Extract imports from #include directives + document links + diagnostics
    let mut imports = HashSet::new();
    let mut links = Vec::new();
    let mut import_diagnostics = Vec::new();

    for item in &ast.items {
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
    }
    IMPORT_GRAPH.update(uri, imports);

    // 3. Single-pass cursor: diagnostics + symbols + folding + id_roles
    let cursor = Cursor::walk(&ast, rope);

    // 4. Merge import diagnostics with cursor diagnostics
    let mut all_diagnostics = cursor.diagnostics;
    all_diagnostics.extend(import_diagnostics);

    // 5. Store results
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
