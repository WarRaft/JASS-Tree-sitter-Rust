use crate::lng::string_colors::{collect_raw_string_colors, tokenize_raw_string};
use crate::lng::wts::kind::Kind;
use crate::lng::wts::trigstr::{TrigstrEntry, TRIGSTR_MAP};
use crate::lsp::color::lsp::ColorInformation;
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
use std::collections::HashMap;
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
    let mut colors: Vec<ColorInformation> = Vec::new();
    let mut trigstr_entries: HashMap<String, TrigstrEntry> = HashMap::new();

    for node in Dfs::new(root) {
        if node.is_missing() {
            let expected = node.kind();
            diagnostics.push(Diagnostic {
                range: node.to_range(&rope),
                message: crate::util::i18n::missing_token(expected),
                severity: Some(DiagnosticSeverity::Error),
                ..Diagnostic::new("wts", "syntax")
            });
            continue;
        }

        if node.is_error() {
            diagnostics.push(Diagnostic {
                range: node.to_range(&rope),
                message: crate::util::i18n::syntax_error().into(),
                severity: Some(DiagnosticSeverity::Error),
                ..Diagnostic::new("wts", "syntax")
            });
        }

        if let Ok(kind) = Kind::try_from(node.grammar_id()) {
            match kind {
                Kind::Header => {
                    let header_range = node.to_range(&rope);
                    if let Some(name_node) = node.child_by_field_name("name") {
                        let name_text = name_node.text(&rope).to_string();
                        let name_range = name_node.to_range(&rope);

                        trigstr_entries.insert(
                            name_text.clone(),
                            TrigstrEntry {
                                header_range: header_range.clone(),
                                name_range: name_range.clone(),
                            },
                        );

                        symbols.push(DocumentSymbol {
                            name: name_text,
                            kind: SymbolKind::String,
                            range: header_range,
                            selection_range: name_range,
                            ..Default::default()
                        });
                    } else {
                        symbols.push(DocumentSymbol {
                            name: "<unnamed>".into(),
                            kind: SymbolKind::String,
                            range: header_range.clone(),
                            selection_range: header_range,
                            ..Default::default()
                        });
                    }
                }
                Kind::StringLiteral => {
                    folding.push(FoldingRange {
                        start_line: node.start_position().row,
                        end_line: node.end_position().row,
                        kind: Some(FoldingRangeKind::Region),
                        ..Default::default()
                    });
                }
                Kind::StringKeyword => {
                    semantic.add_node(&node, &rope, TokenKind::Keyword, 0u32);
                }
                Kind::Identifier => {
                    semantic.add_node(&node, &rope, TokenKind::Number, 0u32);
                }
                Kind::Comment => {
                    semantic.add_node(&node, &rope, TokenKind::Comment, 0u32);
                }
                Kind::LeftBrace | Kind::RightBrace => {
                    semantic.add_node(&node, &rope, TokenKind::Operator, 0u32);
                }
                Kind::StringText => {
                    tokenize_raw_string(&node, &rope, &mut semantic);
                    colors.extend(collect_raw_string_colors(&node, &rope));
                }
                _ => {}
            }
        }
    }

    // Store TRIGSTR map for this WTS file.
    TRIGSTR_MAP.insert(uri.clone(), trigstr_entries);

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
        colors,
    });
    FILE_STORE.insert(uri.clone(), snapshot);

    Ok(())
}

