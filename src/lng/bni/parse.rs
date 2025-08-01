use crate::lng::bni::kind::Kind;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity, DocumentDiagnosticReport};
use crate::lsp::diagnostic::uri_map::URI_MAP as DIAGNOSTIC_URI_MAP;
use crate::lsp::document_symbol::lsp::DocumentSymbol;
use crate::lsp::document_symbol::uri_map::URI_MAP as SYMBOL_URI_MAP;
use crate::lsp::semantic::hub::Hub;
use crate::lsp::semantic::lsp::Kind as TokenKind;
use crate::lsp::semantic::uri_map::URI_MAP as SEMANTIC_URI_MAP;
use crate::util::node_ext::NodeExt;
use crate::util::uri_map::TREE_MAP;
use tree_sitter::Node;
use url::Url;

pub async fn parse(uri: &Url) {
    let tree = {
        let tree_map = TREE_MAP.lock().await;
        match tree_map.get(uri) {
            Some(Some(tree)) => tree.clone(),
            _ => return,
        }
    };

    let mut semantic_map = SEMANTIC_URI_MAP.lock().await;
    let semantic = semantic_map
        .entry(uri.clone())
        .or_insert_with(Hub::new)
        .clear();

    let mut diagnostic_map = DIAGNOSTIC_URI_MAP.lock().await;

    let mut report = DocumentDiagnosticReport::Full {
        result_id: None,
        items: vec![],
        related_documents: None,
    };

    let items = match &mut report {
        DocumentDiagnosticReport::Full { items, .. } => items,
        _ => unreachable!("Expected Full report"),
    };

    let mut symbol_map = SYMBOL_URI_MAP.lock().await;

    let symbols: Vec<DocumentSymbol> = Vec::new();

    let mut current_section: Option<Node> = None;
    let mut current_item: Option<Node> = None;

    let mut stack = vec![tree.root_node().walk()];
    while let Some(mut cursor) = stack.pop() {
        let node = cursor.node();

        if node.is_missing() {
            let expected = node.kind();
            let field = cursor.field_name().unwrap_or("?");
            items.push(Diagnostic {
                range: node.range_lsp(),
                message: format!("Missing `{}` in field `{}`", expected, field),
                severity: Some(DiagnosticSeverity::Error),
                ..Default::default()
            });
            continue;
        }

        if node.is_error() {
            items.push(Diagnostic {
                range: node.range_lsp(),
                message: "Syntax error".into(),
                severity: Some(DiagnosticSeverity::Error),
                ..Default::default()
            });
        }

        if let Ok(kind) = Kind::try_from(node.grammar_id()) {
            match kind {
                Kind::Section => {
                    current_section = Some(node);
                    current_item = None;
                }
                Kind::Item => {
                    current_item = Some(node);
                }
                Kind::Comment => {
                    current_item = None;
                }
                Kind::LeftBracket | Kind::RightBracket | Kind::Equal | Kind::Comma => {
                    semantic.add_node(&node, TokenKind::Operator, 0u32);
                }
                Kind::SectionName => {
                    semantic.add_node(&node, TokenKind::Keyword, 0u32);
                }
                Kind::Key => {
                    semantic.add_node(&node, TokenKind::Function, 0u32);
                }
                Kind::QuotedString | Kind::UnquotedString => {
                    semantic.add_node(&node, TokenKind::String, 0u32);
                }
                Kind::Int | Kind::Float => {
                    semantic.add_node(&node, TokenKind::Number, 0u32);
                }
                Kind::LineComment => {
                    semantic.add_node(&node, TokenKind::Comment, 0u32);
                }
                _ => {}
            }
        }

        if cursor.goto_first_child() {
            let mut child = cursor.clone();
            loop {
                stack.push(child.clone());
                if !cursor.goto_next_sibling() {
                    break;
                }
                child = cursor.clone();
            }
            cursor.goto_parent();
        }
    }

    diagnostic_map.insert(uri.clone(), report);
}
