use crate::lng::bni::kind::Kind;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity};
use crate::lsp::document_symbol::lsp::{DocumentSymbol, SymbolKind};
use crate::lsp::folding::lsp::{FoldingRange, FoldingRangeKind};
use crate::lsp::ref_map::RefMap;
use crate::http::semantic::hub::Hub;
use crate::http::semantic::token::Kind as TokenKind;
use crate::util::dfs_node::Dfs;
use crate::util::file_store::{ParseSnapshot, FILE_STORE};
use crate::util::roper::node::NodeExt;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::tree_map::TREE_MAP;
use std::error::Error;
use std::sync::Arc;
use url::Url;

pub async fn parse(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    _parse(uri)
}

/// Parse + refresh all open editors.
pub async fn parse_and_notify(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    parse(uri).await?;
    crate::util::file_store::send_refresh_all().await;
    Ok(())
}

fn _parse(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Clone owned data and drop DashMap guards immediately to avoid
    // holding read-locks across the whole parse (which would block
    // concurrent apply_edits / insert calls).
    let rope = ROPE_MAP
        .get(uri)
        .map(|r| r.value().clone())
        .ok_or("no rope")?;
    let tree = TREE_MAP
        .get(uri)
        .map(|t| t.value().clone())
        .ok_or("no tree")?;

    let root = tree.root_node();

    let mut semantic = Hub::default();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut symbols: Vec<DocumentSymbol> = Vec::new();
    let mut folding: Vec<FoldingRange> = Vec::new();

    let mut current_symbol: Option<DocumentSymbol> = None;
    let mut current_folding: Option<FoldingRange> = None;

    for node in Dfs::new(root) {
        if node.is_missing() {
            let expected = node.kind();
            diagnostics.push(Diagnostic {
                range: node.to_range(&rope),
                message: crate::util::i18n::missing_token(expected),
                severity: Some(DiagnosticSeverity::Error),
                ..Diagnostic::new("bni", "syntax")
            });
            continue;
        }

        if node.is_error() {
            diagnostics.push(Diagnostic {
                range: node.to_range(&rope),
                message: crate::util::i18n::syntax_error().into(),
                severity: Some(DiagnosticSeverity::Error),
                ..Diagnostic::new("bni", "syntax")
            });
        }

        if let Ok(kind) = Kind::try_from(node.grammar_id()) {
            match kind {
                Kind::Section => {
                    if let Some(prev) = current_symbol.take() {
                        symbols.push(prev);
                    }
                    current_symbol = Some(DocumentSymbol {
                        name: "<unnamed>".into(),
                        kind: SymbolKind::Namespace,
                        range: node.to_range(&rope),
                        selection_range: node.to_range(&rope),
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
                    semantic.add_node(&node, &rope, TokenKind::Keyword, 0u32);
                    if let Some(symbol) = current_symbol.as_mut() {
                        symbol.name = node.text(&rope).to_string();
                        symbol.selection_range = node.to_range(&rope);
                    }
                }
                Kind::Item => {
                    if let Some(symbol) = current_symbol.as_mut() {
                        symbol.range.end = node.end_position().into();
                    }
                    if let Some(folding) = current_folding.as_mut() {
                        folding.end_line = node.end_position().row;
                    }
                }
                Kind::Comment => {
                    if let Some(symbol) = current_symbol.as_mut() {
                        symbol.range.end = node.end_position().into();
                    }
                    if let Some(folding) = current_folding.as_mut() {
                        folding.end_line = node.end_position().row;
                    }
                }
                Kind::LBracket | Kind::RightBracket | Kind::Equal | Kind::Comma
                | Kind::DoubleQuote => {
                    semantic.add_node(&node, &rope, TokenKind::Operator, 0u32);
                }
                Kind::Key => {
                    semantic.add_node(&node, &rope, TokenKind::Function, 0u32);
                }
                Kind::QuotedString | Kind::UnquotedString
                | Kind::StringContent => {
                    semantic.add_node(&node, &rope, TokenKind::String, 0u32);
                }
                Kind::Int | Kind::Float => {
                    semantic.add_node(&node, &rope, TokenKind::Number, 0u32);
                }
                Kind::LineComment => {
                    semantic.add_node(&node, &rope, TokenKind::Comment, 0u32);
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

    // Store everything atomically in FILE_STORE.
    let snapshot = Arc::new(ParseSnapshot {
        folding,
        symbols,
        semantic: std::sync::RwLock::new(semantic),
        diagnostics,
        links: Vec::new(),
        ref_map: RefMap::default(),
        file_symbols: Default::default(),
        _type_map: Default::default(),
        type_hints: Vec::new(),
        ujapi_hints: Vec::new(),
        func_decl_keys: Default::default(),
        colors: Vec::new(),
    });
    FILE_STORE.insert(uri.clone(), snapshot);

    Ok(())
}
