use crate::lng::slk::kind::Kind;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity};
use crate::lsp::document_symbol::lsp::{DocumentSymbol, SymbolKind};
use crate::lsp::folding::lsp::{FoldingRange, FoldingRangeKind};
use crate::lsp::ref_map::RefMap;
use crate::lsp::semantic::hub::Hub;
use crate::lsp::semantic::lsp::Kind as TokenKind;
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

    for node in Dfs::new(root) {
        if node.is_missing() {
            let expected = node.kind();
            diagnostics.push(Diagnostic {
                range: node.to_range(&rope),
                message: crate::util::i18n::missing_token(expected),
                severity: Some(DiagnosticSeverity::Error),
                ..Diagnostic::new("slk", "syntax")
            });
            continue;
        }

        if node.is_error() {
            diagnostics.push(Diagnostic {
                range: node.to_range(&rope),
                message: crate::util::i18n::syntax_error().into(),
                severity: Some(DiagnosticSeverity::Error),
                ..Diagnostic::new("slk", "syntax")
            });
        }

        if let Ok(kind) = Kind::try_from(node.grammar_id()) {
            match kind {
                // Record type keywords: ID, B, C, F, O, P
                Kind::IdKeyword
                | Kind::BKeyword
                | Kind::CKeyword
                | Kind::FKeyword
                | Kind::OKeyword
                | Kind::PKeyword => {
                    semantic.add_node(&node, &rope, TokenKind::Keyword, 0u32);
                }

                // Field tag (e.g. X, Y, K)
                Kind::FieldTag => {
                    semantic.add_node(&node, &rope, TokenKind::Parameter, 0u32);
                }

                // Field value
                Kind::FieldValue => {
                    semantic.add_node(&node, &rope, TokenKind::String, 0u32);
                }

                // Semicolons
                Kind::Semicolon => {
                    semantic.add_node(&node, &rope, TokenKind::Operator, 0u32);
                }

                // Records as document symbols
                Kind::IdRecord => {
                    symbols.push(DocumentSymbol {
                        name: "ID".into(),
                        kind: SymbolKind::Struct,
                        range: node.to_range(&rope),
                        selection_range: node.to_range(&rope),
                        ..Default::default()
                    });
                    folding.push(FoldingRange {
                        start_line: node.start_position().row,
                        end_line: node.end_position().row,
                        kind: Some(FoldingRangeKind::Region),
                        ..Default::default()
                    });
                }
                Kind::BRecord => {
                    symbols.push(DocumentSymbol {
                        name: "B".into(),
                        kind: SymbolKind::Struct,
                        range: node.to_range(&rope),
                        selection_range: node.to_range(&rope),
                        ..Default::default()
                    });
                }
                Kind::CRecord => {
                    symbols.push(DocumentSymbol {
                        name: "C".into(),
                        kind: SymbolKind::Field,
                        range: node.to_range(&rope),
                        selection_range: node.to_range(&rope),
                        ..Default::default()
                    });
                }
                Kind::FRecord => {
                    symbols.push(DocumentSymbol {
                        name: "F".into(),
                        kind: SymbolKind::Field,
                        range: node.to_range(&rope),
                        selection_range: node.to_range(&rope),
                        ..Default::default()
                    });
                }
                Kind::ORecord => {
                    symbols.push(DocumentSymbol {
                        name: "O".into(),
                        kind: SymbolKind::Field,
                        range: node.to_range(&rope),
                        selection_range: node.to_range(&rope),
                        ..Default::default()
                    });
                }
                Kind::PRecord => {
                    symbols.push(DocumentSymbol {
                        name: "P".into(),
                        kind: SymbolKind::Field,
                        range: node.to_range(&rope),
                        selection_range: node.to_range(&rope),
                        ..Default::default()
                    });
                }
                Kind::EndRecord => {
                    semantic.add_node(&node, &rope, TokenKind::Keyword, 0u32);
                    symbols.push(DocumentSymbol {
                        name: "E".into(),
                        kind: SymbolKind::Struct,
                        range: node.to_range(&rope),
                        selection_range: node.to_range(&rope),
                        ..Default::default()
                    });
                }

                Kind::TailContent => {
                    semantic.add_node(&node, &rope, TokenKind::Comment, 0u32);
                }

                _ => {}
            }
        }
    }

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

