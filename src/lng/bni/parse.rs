use crate::lng::bni::kind::Kind;
use crate::lng::bni::uri_map::TREE_MAP;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity, DocumentDiagnosticReport};
use crate::lsp::diagnostic::uri_map::URI_MAP as DIAGNOSTIC_URI_MAP;
use crate::lsp::document_symbol::lsp::{DocumentSymbol, SymbolKind};
use crate::lsp::document_symbol::uri_map::URI_MAP as SYMBOL_URI_MAP;
use crate::lsp::folding::lsp::{FoldingRange, FoldingRangeKind};
use crate::lsp::folding::uri_map::URI_MAP as FOLDING_URI_MAP;
use crate::lsp::semantic::hub::Hub;
use crate::lsp::semantic::lsp::Kind as TokenKind;
use crate::lsp::semantic::uri_map::URI_MAP as SEMANTIC_URI_MAP;
use crate::util::dfs_node::Dfs;
use crate::util::uri_map::LINE_LIST_MAP;
use std::error::Error;
use url::Url;

pub async fn parse(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    {
        let tree_map = TREE_MAP.lock().await;
        let mut line_list_map = LINE_LIST_MAP.lock().await;
        let mut semantic_map = SEMANTIC_URI_MAP.lock().await;
        let mut diagnostic_map = DIAGNOSTIC_URI_MAP.lock().await;
        let mut symbol_map = SYMBOL_URI_MAP.lock().await;
        let mut folding_map = FOLDING_URI_MAP.lock().await;

        let line_list = line_list_map.get_mut(&uri).ok_or("no line list")?;
        let semantic = semantic_map
            .entry(uri.clone())
            .or_insert_with(Hub::new)
            .clear();

        let mut report = DocumentDiagnosticReport::Full {
            result_id: None,
            items: vec![],
            related_documents: None,
        };

        let diagnostic = match &mut report {
            DocumentDiagnosticReport::Full { items, .. } => items,
            _ => unreachable!("Expected Full report"),
        };

        let mut symbols: Vec<DocumentSymbol> = Vec::new();
        let mut folding: Vec<FoldingRange> = Vec::new();

        //let mut current_section: Option<Node> = None;
        //let mut current_item: Option<Node> = None;
        let mut current_symbol: Option<DocumentSymbol> = None;
        let mut current_folding: Option<FoldingRange> = None;

        let root = tree_map.get(&uri).ok_or("no tree")?.root_node();
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
                        //current_section = Some(node);
                        //current_item = None;
                        if let Some(prev) = current_symbol.take() {
                            symbols.push(prev);
                        }
                        current_symbol = Some(DocumentSymbol {
                            name: "<unnamed>".into(),
                            kind: SymbolKind::Namespace,
                            range: node.into(),
                            selection_range: node.into(),
                            ..Default::default()
                        });

                        if let Some(prev) = current_folding.take() {
                            folding.push(prev);
                        }
                        current_folding = Some(FoldingRange {
                            start_line: node.start_position().row,
                            end_line: node.start_position().row,
                            kind: Some(FoldingRangeKind::Region),
                            ..Default::default()
                        })
                    }
                    Kind::SectionName => {
                        semantic.add_node(&node, TokenKind::Keyword, 0u32);
                        if let Some(symbol) = current_symbol.as_mut() {
                            symbol.name = line_list.node_text(&node).to_string();
                            symbol.selection_range = node.into();
                        }
                    }
                    Kind::Item => {
                        //current_item = Some(node);
                        if let Some(symbol) = current_symbol.as_mut() {
                            symbol.range.end = node.end_position().into();
                        }
                        if let Some(folding) = current_folding.as_mut() {
                            folding.end_line = node.end_position().row;
                        }
                    }
                    Kind::Comment => {
                        //current_item = None;
                        if let Some(symbol) = current_symbol.as_mut() {
                            symbol.range.end = node.end_position().into();
                        }
                        if let Some(folding) = current_folding.as_mut() {
                            folding.end_line = node.end_position().row;
                        }
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
        if let Some(prev) = current_folding {
            folding.push(prev);
        }

        folding_map.insert(uri.clone(), folding);
        symbol_map.insert(uri.clone(), symbols);
        diagnostic_map.insert(uri.clone(), report);
    }
    Ok(())
}
