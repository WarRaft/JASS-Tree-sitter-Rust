use crate::lng::bni::kind::Kind;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity, DocumentDiagnosticReport};
use crate::lsp::diagnostic::uri_map::URI_MAP;
use crate::lsp::position::Position;
use crate::lsp::range::Range;
use crate::lsp::semantic::hub::Hub;
use crate::lsp::semantic::lsp::Kind as TokenKind;
use crate::lsp::semantic::uri_map::URI_MAP as SEMANTIC_URI_MAP;
use crate::util::uri_map::TREE_MAP;
use log::error;
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

    let mut diagnostic_map = URI_MAP.lock().await;

    let mut report = DocumentDiagnosticReport::Full {
        result_id: None,
        items: vec![],
        related_documents: None,
    };

    let items = match &mut report {
        DocumentDiagnosticReport::Full { items, .. } => items,
        _ => unreachable!("Expected Full report"),
    };

    fn walk(
        node: Node,
        semantic: &mut Hub,
        diagnostics: &mut Vec<Diagnostic>,
        current_section: Option<Node>,
        current_item: Option<Node>,
    ) {
        if node.is_missing() {
            diagnostics.push(Diagnostic {
                range: to_range(&node),
                message: format!("Missing node: expected `{}`", node.kind()),
                severity: Some(DiagnosticSeverity::Error),
                ..Default::default()
            });
            return;
        }

        if node.is_error() {
            diagnostics.push(Diagnostic {
                range: to_range(&node),
                message: "Syntax error".into(),
                severity: Some(DiagnosticSeverity::Error),
                ..Default::default()
            });
        }

        let mut new_section = current_section;
        let mut new_item = current_item;

        if !node.is_error() {
            let kind = match Kind::try_from(node.grammar_id()) {
                Ok(k) => k,
                Err(_) => {
                    error!("Unknown error kind {:?}", node.kind());
                    return;
                }
            };

            new_section = match kind {
                Kind::Section => Some(node),
                _ => current_section,
            };

            new_item = match kind {
                Kind::Item => Some(node),
                Kind::Section | Kind::Comment => None,
                _ => current_item,
            };

            match kind {
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

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                walk(child, semantic, diagnostics, new_section, new_item);
            }
        }
    }

    let root = tree.root_node();
    for i in 0..root.child_count() {
        if let Some(child) = root.child(i) {
            walk(child, semantic, items, None, None);
        }
    }

    diagnostic_map.insert(uri.clone(), report);
}

fn to_range(node: &Node) -> Range {
    let s = node.start_position();
    let e = node.end_position();
    Range {
        start: Position {
            line: s.row,
            character: s.column,
        },
        end: Position {
            line: e.row,
            character: e.column,
        },
    }
}
