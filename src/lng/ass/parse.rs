use crate::lng::ass::ast::build_ast;
use crate::lng::ass::cursor::Cursor;
use crate::lng::ass::uri_map::TREE_MAP;
use crate::lsp::diagnostic::lsp::DocumentDiagnosticReport;
use crate::lsp::diagnostic::uri_map::URI_MAP as DIAGNOSTIC_URI_MAP;
use crate::lsp::document_symbol::uri_map::URI_MAP as SYMBOL_URI_MAP;
use crate::lsp::folding::uri_map::URI_MAP as FOLDING_URI_MAP;
use crate::lsp::semantic::uri_map::URI_MAP as SEMANTIC_URI_MAP;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_lock::uri_unlock;
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

    // 2. Single-pass cursor: diagnostics + symbols + folding + id_roles
    let cursor = Cursor::walk(&ast, rope);

    // 3. Store results
    let report = DocumentDiagnosticReport::Full {
        result_id: None,
        items: cursor.diagnostics,
        related_documents: None,
    };

    FOLDING_URI_MAP.insert(uri.clone(), cursor.folding);
    SYMBOL_URI_MAP.insert(uri.clone(), cursor.symbols);
    DIAGNOSTIC_URI_MAP.insert(uri.clone(), report);
    SEMANTIC_URI_MAP.insert(uri.clone(), cursor.semantic);

    Ok(())
}
