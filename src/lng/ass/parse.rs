use crate::lng::ass::kind::Kind;
use crate::lng::ass::uri_map::TREE_MAP;
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
use crate::util::roper::node::NodeExt;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_lock::uri_unlock;
use lapce_xi_rope::Rope;
use std::error::Error;
use url::Url;

pub async fn parse(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    let result = _parse(uri);
    uri_unlock(uri);
    result
}

fn _parse(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    {
        let rope_entry = ROPE_MAP.get(&uri.clone()).ok_or("no rope")?;
        let rope: &Rope = rope_entry.value();

        let mut semantic = Hub::default();
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

        let tree_entry = TREE_MAP.get(&uri.clone()).ok_or("no tree")?;
        let root = tree_entry.value().root_node();

        for node in Dfs::new(root) {
            if node.is_missing() {
                let expected = node.kind();
                diagnostic.push(Diagnostic {
                    range: node.to_range(rope),
                    message: format!("Missing `{}`", expected),
                    severity: Some(DiagnosticSeverity::Error),
                    ..Default::default()
                });
                continue;
            }

            if node.is_error() {
                diagnostic.push(Diagnostic {
                    range: node.to_range(rope),
                    message: "Syntax error".into(),
                    severity: Some(DiagnosticSeverity::Error),
                    ..Default::default()
                });
            }

            if let Ok(kind) = Kind::try_from(node.grammar_id()) {
                match kind {
                    // ── Document symbols ──────────────────────────────────────
                    Kind::FunctionDeclaration => {
                        let name = node
                            .child_by_field_id(19) // field_name
                            .map(|n| n.text(rope).to_string())
                            .unwrap_or_else(|| "<unnamed>".into());
                        symbols.push(DocumentSymbol {
                            name,
                            kind: SymbolKind::Function,
                            range: node.to_range(rope),
                            selection_range: node
                                .child_by_field_id(19)
                                .map(|n| n.to_range(rope))
                                .unwrap_or_else(|| node.to_range(rope)),
                            ..Default::default()
                        });
                        folding.push(FoldingRange {
                            start_line: node.start_position().row,
                            end_line: node.end_position().row,
                            kind: Some(FoldingRangeKind::Region),
                            ..Default::default()
                        });
                    }
                    Kind::ClassDeclaration => {
                        let name = node
                            .child_by_field_id(19)
                            .map(|n| n.text(rope).to_string())
                            .unwrap_or_else(|| "<unnamed>".into());
                        symbols.push(DocumentSymbol {
                            name,
                            kind: SymbolKind::Class,
                            range: node.to_range(rope),
                            selection_range: node
                                .child_by_field_id(19)
                                .map(|n| n.to_range(rope))
                                .unwrap_or_else(|| node.to_range(rope)),
                            ..Default::default()
                        });
                        folding.push(FoldingRange {
                            start_line: node.start_position().row,
                            end_line: node.end_position().row,
                            kind: Some(FoldingRangeKind::Region),
                            ..Default::default()
                        });
                    }
                    Kind::InterfaceDeclaration => {
                        let name = node
                            .child_by_field_id(19)
                            .map(|n| n.text(rope).to_string())
                            .unwrap_or_else(|| "<unnamed>".into());
                        symbols.push(DocumentSymbol {
                            name,
                            kind: SymbolKind::Interface,
                            range: node.to_range(rope),
                            selection_range: node
                                .child_by_field_id(19)
                                .map(|n| n.to_range(rope))
                                .unwrap_or_else(|| node.to_range(rope)),
                            ..Default::default()
                        });
                        folding.push(FoldingRange {
                            start_line: node.start_position().row,
                            end_line: node.end_position().row,
                            kind: Some(FoldingRangeKind::Region),
                            ..Default::default()
                        });
                    }
                    Kind::EnumDeclaration => {
                        let name = node
                            .child_by_field_id(19)
                            .map(|n| n.text(rope).to_string())
                            .unwrap_or_else(|| "<unnamed>".into());
                        symbols.push(DocumentSymbol {
                            name,
                            kind: SymbolKind::Enum,
                            range: node.to_range(rope),
                            selection_range: node
                                .child_by_field_id(19)
                                .map(|n| n.to_range(rope))
                                .unwrap_or_else(|| node.to_range(rope)),
                            ..Default::default()
                        });
                        folding.push(FoldingRange {
                            start_line: node.start_position().row,
                            end_line: node.end_position().row,
                            kind: Some(FoldingRangeKind::Region),
                            ..Default::default()
                        });
                    }
                    Kind::NamespaceDeclaration => {
                        let name = node
                            .child_by_field_id(19)
                            .map(|n| n.text(rope).to_string())
                            .unwrap_or_else(|| "<unnamed>".into());
                        symbols.push(DocumentSymbol {
                            name,
                            kind: SymbolKind::Namespace,
                            range: node.to_range(rope),
                            selection_range: node
                                .child_by_field_id(19)
                                .map(|n| n.to_range(rope))
                                .unwrap_or_else(|| node.to_range(rope)),
                            ..Default::default()
                        });
                        folding.push(FoldingRange {
                            start_line: node.start_position().row,
                            end_line: node.end_position().row,
                            kind: Some(FoldingRangeKind::Region),
                            ..Default::default()
                        });
                    }
                    Kind::MixinDeclaration => {
                        let name = node
                            .child_by_field_id(19)
                            .map(|n| n.text(rope).to_string())
                            .unwrap_or_else(|| "<unnamed>".into());
                        symbols.push(DocumentSymbol {
                            name,
                            kind: SymbolKind::Class,
                            range: node.to_range(rope),
                            selection_range: node
                                .child_by_field_id(19)
                                .map(|n| n.to_range(rope))
                                .unwrap_or_else(|| node.to_range(rope)),
                            ..Default::default()
                        });
                        folding.push(FoldingRange {
                            start_line: node.start_position().row,
                            end_line: node.end_position().row,
                            kind: Some(FoldingRangeKind::Region),
                            ..Default::default()
                        });
                    }

                    // ── Folding only ─────────────────────────────────────────
                    Kind::Block | Kind::IfStatement | Kind::WhileStatement
                    | Kind::DoWhileStatement | Kind::ForStatement | Kind::ForeachStatement
                    | Kind::SwitchStatement | Kind::TryStatement => {
                        folding.push(FoldingRange {
                            start_line: node.start_position().row,
                            end_line: node.end_position().row,
                            kind: Some(FoldingRangeKind::Region),
                            ..Default::default()
                        });
                    }

                    // ── Semantic tokens ───────────────────────────────────────
                    Kind::Comment | Kind::BlockComment => {
                        semantic.add_node(&node, rope, TokenKind::Comment, 0u32);
                    }
                    Kind::StringLiteral => {
                        semantic.add_node(&node, rope, TokenKind::String, 0u32);
                    }
                    Kind::IntegerLiteral | Kind::HexLiteral | Kind::BitsLiteral
                    | Kind::FloatLiteral | Kind::NullLiteral | Kind::True | Kind::False => {
                        semantic.add_node(&node, rope, TokenKind::Number, 0u32);
                    }
                    Kind::Identifier => {
                        // Determine role from parent
                        let token_kind = if let Some(parent) = node.parent() {
                            match Kind::try_from(parent.grammar_id()) {
                                Ok(Kind::FunctionCall) => TokenKind::Function,
                                Ok(Kind::FunctionDeclaration) => TokenKind::Function,
                                Ok(Kind::ClassDeclaration) | Ok(Kind::InterfaceDeclaration)
                                | Ok(Kind::MixinDeclaration) | Ok(Kind::EnumDeclaration) => {
                                    TokenKind::Type
                                }
                                Ok(Kind::Type) | Ok(Kind::PrimitiveType) => TokenKind::Type,
                                Ok(Kind::Parameter) => TokenKind::Parameter,
                                Ok(Kind::EnumMember) => TokenKind::EnumMember,
                                Ok(Kind::NamespaceDeclaration) | Ok(Kind::NamespaceAccess) => {
                                    TokenKind::Namespace
                                }
                                Ok(Kind::MemberAccess) => TokenKind::Property,
                                _ => TokenKind::Variable,
                            }
                        } else {
                            TokenKind::Variable
                        };
                        semantic.add_node(&node, rope, token_kind, 0u32);
                    }

                    // keywords
                    Kind::HashInclude | Kind::Import | Kind::From | Kind::Namespace
                    | Kind::Typedef | Kind::Shared | Kind::Funcdef | Kind::External
                    | Kind::Enum | Kind::Interface | Kind::Mixin | Kind::Abstract
                    | Kind::Final | Kind::Class | Kind::Private | Kind::Protected
                    | Kind::Public | Kind::Override | Kind::Explicit | Kind::Const
                    | Kind::Delete | Kind::If | Kind::Else | Kind::While | Kind::Do
                    | Kind::For | Kind::In | Kind::Switch | Kind::Case | Kind::Default
                    | Kind::Return | Kind::Break | Kind::Continue | Kind::Try | Kind::Catch
                    | Kind::Throw | Kind::Cast | Kind::OpImplCast | Kind::Function
                    | Kind::New | Kind::Is | Kind::Not | Kind::And | Kind::Or | Kind::Xor => {
                        semantic.add_node(&node, rope, TokenKind::Keyword, 0u32);
                    }

                    // primitive type keywords
                    Kind::Void | Kind::Int | Kind::Int8 | Kind::Int16 | Kind::Int32
                    | Kind::Int64 | Kind::Uint | Kind::Uint8 | Kind::Uint16 | Kind::Uint32
                    | Kind::Uint64 | Kind::Float | Kind::Double | Kind::Bool | Kind::StringKw
                    | Kind::Auto => {
                        semantic.add_node(&node, rope, TokenKind::Type, 0u32);
                    }

                    // operators
                    Kind::LeftParen | Kind::RightParen | Kind::LeftBrace | Kind::RightBrace
                    | Kind::LeftBracket | Kind::RightBracket | Kind::Comma | Kind::Equal
                    | Kind::Colon | Kind::Tilde | Kind::Question | Kind::Dot | Kind::AtDot
                    | Kind::ColonColon | Kind::At | Kind::Semicolon
                    | Kind::PlusEq | Kind::MinusEq | Kind::StarEq | Kind::SlashEq
                    | Kind::PercentEq | Kind::StarStarEq | Kind::AmpEq | Kind::PipeEq
                    | Kind::CaretEq | Kind::LtLtEq | Kind::GtGtEq | Kind::GtGtGtEq
                    | Kind::PipePipe | Kind::AmpAmp | Kind::Pipe | Kind::Caret | Kind::Amp
                    | Kind::EqEq | Kind::BangEq | Kind::Bang | Kind::Lt | Kind::Gt
                    | Kind::LtEq | Kind::GtEq | Kind::LtLt | Kind::GtGt | Kind::GtGtGt
                    | Kind::Plus | Kind::Minus | Kind::Star | Kind::Slash | Kind::Percent
                    | Kind::StarStar | Kind::PlusPlus | Kind::MinusMinus => {
                        semantic.add_node(&node, rope, TokenKind::Operator, 0u32);
                    }

                    Kind::ThisExpression | Kind::SuperExpression => {
                        semantic.add_node(&node, rope, TokenKind::Keyword, 0u32);
                    }

                    _ => {}
                }
            }
        }

        FOLDING_URI_MAP.insert(uri.clone(), folding);
        SYMBOL_URI_MAP.insert(uri.clone(), symbols);
        DIAGNOSTIC_URI_MAP.insert(uri.clone(), report);
        SEMANTIC_URI_MAP.insert(uri.clone(), semantic);
    }
    Ok(())
}

