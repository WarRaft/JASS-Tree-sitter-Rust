use crate::lng::jass::ast::build_ast;
use crate::lng::jass::cursor::Cursor;
use crate::lng::jass::uri_map::TREE_MAP;
use crate::lsp::diagnostic::lsp::DocumentDiagnosticReport;
use crate::lsp::diagnostic::uri_map::URI_MAP as DIAGNOSTIC_URI_MAP;
use crate::lsp::document_symbol::uri_map::URI_MAP as SYMBOL_URI_MAP;
use crate::lsp::folding::uri_map::URI_MAP as FOLDING_URI_MAP;
use crate::lsp::semantic::uri_map::URI_MAP as SEMANTIC_URI_MAP;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_lock::uri_unlock;
use lapce_xi_rope::Rope;
use std::error::Error;
use url::Url;

// ─── Main parse entry point ─────────────────────────────────────────────────

pub async fn parse(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    {
        let rope_entry = ROPE_MAP.get(&uri.clone()).ok_or("no rope")?;
        let rope: &Rope = rope_entry.value();

        let tree_entry = TREE_MAP.get(&uri.clone()).ok_or("no tree")?;
        let root = tree_entry.value().root_node();

        // 1. Build AST from CST
        let ast = build_ast(root);

        // 2. Single-pass cursor: diagnostics + symbols + folding + id_roles + scopes
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

        uri_unlock(uri);
    }
    Ok(())
}
