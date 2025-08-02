use crate::lng::bni::kind::Kind;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity, DocumentDiagnosticReport};
use crate::lsp::diagnostic::uri_map::URI_MAP as DIAGNOSTIC_URI_MAP;
use crate::lsp::document_symbol::lsp::{DocumentSymbol, SymbolKind};
use crate::lsp::document_symbol::uri_map::URI_MAP as SYMBOL_URI_MAP;
use crate::lsp::semantic::hub::Hub;
use crate::lsp::semantic::lsp::Kind as TokenKind;
use crate::lsp::semantic::uri_map::URI_MAP as SEMANTIC_URI_MAP;
use crate::util::line_list::LineList;
use crate::util::dfs_node::Dfs;
use crate::util::uri_map::TREE_MAP;
use log::info;
use url::Url;

pub async fn parse(uri: &Url, line_list: &LineList) {
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

    let diagnostic = match &mut report {
        DocumentDiagnosticReport::Full { items, .. } => items,
        _ => unreachable!("Expected Full report"),
    };

    let mut symbol_map = SYMBOL_URI_MAP.lock().await;
    let mut symbols: Vec<DocumentSymbol> = Vec::new();

    //let mut current_section: Option<Node> = None;
    //let mut current_item: Option<Node> = None;
    let mut current_symbol: Option<DocumentSymbol> = None;

    let root = tree.root_node();
    for node in Dfs::new(root) {
        if node.is_missing() {
            let expected = node.kind();
            diagnostic.push(Diagnostic {
                range: node.into(),
                message: format!("Missing `{}`", expected),
                severity: Some(DiagnosticSeverity::Error),
                ..Default::default()
            });
            continue;
        }

        if node.is_error() {
            diagnostic.push(Diagnostic {
                range: node.into(),
                message: "Syntax error".into(),
                severity: Some(DiagnosticSeverity::Error),
                ..Default::default()
            });
        }

        if let Ok(kind) = Kind::try_from(node.grammar_id()) {
            match kind {
                Kind::Section => {
                    info!("SECTION");
                    if let Some(prev) = current_symbol.take() {
                        symbols.push(prev);
                    }
                    //current_section = Some(node);
                    //current_item = None;
                    current_symbol = Some(DocumentSymbol {
                        name: "<unnamed>".into(),
                        kind: SymbolKind::Namespace,
                        range: node.into(),
                        selection_range: node.into(),
                        ..Default::default()
                    });
                }
                Kind::SectionName => {
                    info!("SECTION NAME");
                    semantic.add_node(&node, TokenKind::Keyword, 0u32);
                    if let Some(symbol) = current_symbol.as_mut() {
                        symbol.name = line_list.node_text(&node).to_string();
                        symbol.selection_range = node.into();
                    }
                }
                Kind::Item => {
                    info!("ITEM");
                    //current_item = Some(node);
                    if let Some(symbol) = current_symbol.as_mut() {
                        symbol.range.end = node.end_position().into();
                    }
                }
                Kind::Comment => {
                    //current_item = None;
                }
                Kind::LeftBracket | Kind::RightBracket | Kind::Equal | Kind::Comma => {
                    semantic.add_node(&node, TokenKind::Operator, 0u32);
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
    }

    if let Some(symbol) = current_symbol {
        symbols.push(symbol);
    }
    symbol_map.insert(uri.clone(), symbols);
    diagnostic_map.insert(uri.clone(), report);
}
