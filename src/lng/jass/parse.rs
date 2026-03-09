use crate::lng::jass::kind::{Field, Kind};
use crate::lng::jass::uri_map::TREE_MAP;
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

const FIELD_NAME: u16 = Field::Name as u16;
const FIELD_TYPE: u16 = Field::Type as u16;
const FIELD_RETURN_TYPE: u16 = Field::ReturnType as u16;

/// Determine the semantic token kind for an `id` node based on its parent context.
fn id_token_kind(node: &tree_sitter::Node) -> TokenKind {
    if let Some(parent) = node.parent() {
        if let Ok(parent_kind) = Kind::try_from(parent.grammar_id()) {
            match parent_kind {
                // function name in declaration / native
                Kind::FunctionStatement | Kind::NativeStatement => {
                    if let Some(name_node) = parent.child_by_field_id(FIELD_NAME) {
                        if name_node.id() == node.id() {
                            return TokenKind::Function;
                        }
                    }
                    if let Some(rt) = parent.child_by_field_id(FIELD_RETURN_TYPE) {
                        if rt.id() == node.id() {
                            return TokenKind::Type;
                        }
                    }
                    return TokenKind::Variable;
                }
                // expr wraps id — check if the expr is the name field of a function_call
                Kind::Expr => {
                    if let Some(grandparent) = parent.parent() {
                        if let Ok(gp_kind) = Kind::try_from(grandparent.grammar_id()) {
                            match gp_kind {
                                Kind::FunctionCall => {
                                    if let Some(name_expr) = grandparent.child_by_field_id(FIELD_NAME) {
                                        if name_expr.id() == parent.id() {
                                            return TokenKind::Function;
                                        }
                                    }
                                    return TokenKind::Variable;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                // type declaration: name → class, base → type
                Kind::TypeStatement => {
                    if let Some(name_node) = parent.child_by_field_id(FIELD_NAME) {
                        if name_node.id() == node.id() {
                            return TokenKind::Class;
                        }
                    }
                    return TokenKind::Type;
                }
                // parameter: type field → Type, name field → Parameter
                Kind::Parameter => {
                    if let Some(type_node) = parent.child_by_field_id(FIELD_TYPE) {
                        if type_node.id() == node.id() {
                            return TokenKind::Type;
                        }
                    }
                    return TokenKind::Parameter;
                }
                // var_stmt: type field → Type (name lives inside var_decl children)
                Kind::VarStmt => {
                    if let Some(type_node) = parent.child_by_field_id(FIELD_TYPE) {
                        if type_node.id() == node.id() {
                            return TokenKind::Type;
                        }
                    }
                    return TokenKind::Variable;
                }
                // var_decl: name field → Variable
                Kind::VarDecl => {
                    return TokenKind::Variable;
                }
                // local statement: type → Type, name → Variable
                Kind::LocalStatement => {
                    if let Some(type_node) = parent.child_by_field_id(FIELD_TYPE) {
                        if type_node.id() == node.id() {
                            return TokenKind::Type;
                        }
                    }
                    return TokenKind::Variable;
                }
                // set statement: variable field → Variable
                Kind::SetStatement => {
                    return TokenKind::Variable;
                }
                _ => {}
            }
        }
    }
    TokenKind::Variable
}

pub async fn parse(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
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

        // Stack for parent symbols that can contain children (function, globals).
        // Each entry: (symbol, node_id) so we know when to pop.
        let mut symbol_stack: Vec<(DocumentSymbol, usize)> = Vec::new();

        // Track consecutive comment lines for comment folding.
        let mut comment_start_line: Option<usize> = None;
        let mut comment_end_line: usize = 0;

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

            // Pop finished parent symbols from the stack.
            while let Some((_, parent_id)) = symbol_stack.last() {
                let mut is_descendant = false;
                let mut p = Some(node);
                while let Some(ancestor) = p {
                    if ancestor.id() == *parent_id {
                        is_descendant = true;
                        break;
                    }
                    p = ancestor.parent();
                }
                if !is_descendant {
                    let (finished, _) = symbol_stack.pop().unwrap();
                    if let Some((parent, _)) = symbol_stack.last_mut() {
                        parent.children.get_or_insert_with(Vec::new).push(finished);
                    } else {
                        symbols.push(finished);
                    }
                } else {
                    break;
                }
            }

            if let Ok(kind) = Kind::try_from(node.grammar_id()) {
                // Flush comment folding range when we see a non-comment node.
                if kind != Kind::Comment {
                    if let Some(start) = comment_start_line.take() {
                        if comment_end_line > start {
                            folding.push(FoldingRange {
                                start_line: start,
                                end_line: comment_end_line,
                                kind: Some(FoldingRangeKind::Comment),
                                ..Default::default()
                            });
                        }
                    }
                }

                match kind {
                    // ── Container symbols (pushed onto stack) ──────────────
                    Kind::FunctionStatement => {
                        let name = node
                            .child_by_field_id(FIELD_NAME)
                            .map(|n| n.text(rope).to_string())
                            .unwrap_or_else(|| "<unnamed>".into());
                        let sel = node
                            .child_by_field_id(FIELD_NAME)
                            .map(|n| n.to_range(rope))
                            .unwrap_or_else(|| node.to_range(rope));

                        symbol_stack.push((
                            DocumentSymbol {
                                name,
                                kind: SymbolKind::Function,
                                range: node.to_range(rope),
                                selection_range: sel,
                                ..Default::default()
                            },
                            node.id(),
                        ));

                        folding.push(FoldingRange {
                            start_line: node.start_position().row,
                            end_line: node.end_position().row,
                            kind: Some(FoldingRangeKind::Region),
                            ..Default::default()
                        });
                    }
                    Kind::GlobalsBlock => {
                        symbol_stack.push((
                            DocumentSymbol {
                                name: "globals".into(),
                                kind: SymbolKind::Namespace,
                                range: node.to_range(rope),
                                selection_range: node.to_range(rope),
                                ..Default::default()
                            },
                            node.id(),
                        ));

                        folding.push(FoldingRange {
                            start_line: node.start_position().row,
                            end_line: node.end_position().row,
                            kind: Some(FoldingRangeKind::Region),
                            ..Default::default()
                        });
                    }

                    // ── Leaf symbols (pushed directly) ────────────────────
                    Kind::NativeStatement => {
                        let name = node
                            .child_by_field_id(FIELD_NAME)
                            .map(|n| n.text(rope).to_string())
                            .unwrap_or_else(|| "<unnamed>".into());
                        let sel = node
                            .child_by_field_id(FIELD_NAME)
                            .map(|n| n.to_range(rope))
                            .unwrap_or_else(|| node.to_range(rope));

                        let sym = DocumentSymbol {
                            name,
                            kind: SymbolKind::Interface,
                            range: node.to_range(rope),
                            selection_range: sel,
                            ..Default::default()
                        };
                        if let Some((parent, _)) = symbol_stack.last_mut() {
                            parent.children.get_or_insert_with(Vec::new).push(sym);
                        } else {
                            symbols.push(sym);
                        }
                    }
                    Kind::TypeStatement => {
                        let name = node
                            .child_by_field_id(FIELD_NAME)
                            .map(|n| n.text(rope).to_string())
                            .unwrap_or_else(|| "<unnamed>".into());
                        let sel = node
                            .child_by_field_id(FIELD_NAME)
                            .map(|n| n.to_range(rope))
                            .unwrap_or_else(|| node.to_range(rope));

                        let sym = DocumentSymbol {
                            name,
                            kind: SymbolKind::Class,
                            range: node.to_range(rope),
                            selection_range: sel,
                            ..Default::default()
                        };
                        if let Some((parent, _)) = symbol_stack.last_mut() {
                            parent.children.get_or_insert_with(Vec::new).push(sym);
                        } else {
                            symbols.push(sym);
                        }
                    }

                    // ── Variable declarations ─────────────────────────────
                    Kind::VarDecl => {
                        let name = node
                            .child_by_field_id(FIELD_NAME)
                            .map(|n| n.text(rope).to_string())
                            .unwrap_or_else(|| "<unnamed>".into());
                        let sel = node
                            .child_by_field_id(FIELD_NAME)
                            .map(|n| n.to_range(rope))
                            .unwrap_or_else(|| node.to_range(rope));

                        // Check if parent var_stmt has 'constant' keyword.
                        let is_constant = node.parent().map_or(false, |p| {
                            (0..p.child_count()).any(|i| {
                                p.child(i as u32).map_or(false, |c| {
                                    Kind::try_from(c.grammar_id()) == Ok(Kind::Constant)
                                })
                            })
                        });

                        let sym = DocumentSymbol {
                            name,
                            kind: if is_constant {
                                SymbolKind::Constant
                            } else {
                                SymbolKind::Variable
                            },
                            range: node.to_range(rope),
                            selection_range: sel,
                            ..Default::default()
                        };
                        if let Some((parent, _)) = symbol_stack.last_mut() {
                            parent.children.get_or_insert_with(Vec::new).push(sym);
                        } else {
                            symbols.push(sym);
                        }
                    }
                    Kind::LocalStatement => {
                        let name = node
                            .child_by_field_id(FIELD_NAME)
                            .map(|n| n.text(rope).to_string())
                            .unwrap_or_else(|| "<unnamed>".into());
                        let sel = node
                            .child_by_field_id(FIELD_NAME)
                            .map(|n| n.to_range(rope))
                            .unwrap_or_else(|| node.to_range(rope));

                        let is_constant = (0..node.child_count()).any(|i| {
                            node.child(i as u32).map_or(false, |c| {
                                Kind::try_from(c.grammar_id()) == Ok(Kind::Constant)
                            })
                        });

                        let sym = DocumentSymbol {
                            name,
                            kind: if is_constant {
                                SymbolKind::Constant
                            } else {
                                SymbolKind::Variable
                            },
                            range: node.to_range(rope),
                            selection_range: sel,
                            ..Default::default()
                        };
                        if let Some((parent, _)) = symbol_stack.last_mut() {
                            parent.children.get_or_insert_with(Vec::new).push(sym);
                        } else {
                            symbols.push(sym);
                        }
                    }

                    // ── Parameters ────────────────────────────────────────
                    Kind::Parameter => {
                        let name = node
                            .child_by_field_id(FIELD_NAME)
                            .map(|n| n.text(rope).to_string())
                            .unwrap_or_else(|| "<unnamed>".into());
                        let sel = node
                            .child_by_field_id(FIELD_NAME)
                            .map(|n| n.to_range(rope))
                            .unwrap_or_else(|| node.to_range(rope));

                        let type_name = node
                            .child_by_field_id(FIELD_TYPE)
                            .map(|n| n.text(rope).to_string());

                        let sym = DocumentSymbol {
                            name,
                            detail: type_name,
                            kind: SymbolKind::Variable,
                            range: node.to_range(rope),
                            selection_range: sel,
                            ..Default::default()
                        };
                        if let Some((parent, _)) = symbol_stack.last_mut() {
                            parent.children.get_or_insert_with(Vec::new).push(sym);
                        } else {
                            symbols.push(sym);
                        }
                    }

                    // ── Folding ranges ────────────────────────────────────
                    Kind::IfStatement | Kind::LoopStatement => {
                        folding.push(FoldingRange {
                            start_line: node.start_position().row,
                            end_line: node.end_position().row,
                            kind: Some(FoldingRangeKind::Region),
                            ..Default::default()
                        });
                    }

                    // ── Identifiers (semantic coloring by context) ────────
                    // `grammar_id()` for `id` nodes returns IdToken (52)
                    // because `id` is an alias for `_id_token` in the grammar.
                    Kind::IdToken => {
                        let token_kind = id_token_kind(&node);
                        semantic.add_node(&node, rope, token_kind, 0u32);
                    }

                    // ── Keywords ──────────────────────────────────────────
                    Kind::Function
                    | Kind::Endfunction
                    | Kind::Native
                    | Kind::Type
                    | Kind::Extends
                    | Kind::Takes
                    | Kind::Returns
                    | Kind::Nothing
                    | Kind::Local
                    | Kind::Set
                    | Kind::Call
                    | Kind::Return
                    | Kind::If
                    | Kind::Then
                    | Kind::Elseif
                    | Kind::Else
                    | Kind::Endif
                    | Kind::Loop
                    | Kind::Endloop
                    | Kind::Exitwhen
                    | Kind::Globals
                    | Kind::Endglobals
                    | Kind::Constant
                    | Kind::Array
                    | Kind::And
                    | Kind::Or
                    | Kind::Not => {
                        semantic.add_node(&node, rope, TokenKind::Keyword, 0u32);
                    }

                    // ── Operators ─────────────────────────────────────────
                    Kind::Equal
                    | Kind::Comma
                    | Kind::LeftParen
                    | Kind::RightParen
                    | Kind::LeftBracket
                    | Kind::RightBracket
                    | Kind::Plus
                    | Kind::Minus
                    | Kind::Star
                    | Kind::Slash
                    | Kind::PlusPlus
                    | Kind::MinusMinus
                    | Kind::Lt
                    | Kind::Gt
                    | Kind::Le
                    | Kind::Ge
                    | Kind::EqEq
                    | Kind::Neq => {
                        semantic.add_node(&node, rope, TokenKind::Operator, 0u32);
                    }

                    // ── Literals ──────────────────────────────────────────
                    Kind::Number | Kind::Float | Kind::Rawcode => {
                        semantic.add_node(&node, rope, TokenKind::Number, 0u32);
                    }
                    Kind::StringLiteral | Kind::StringContent | Kind::Quote => {
                        semantic.add_node(&node, rope, TokenKind::String, 0u32);
                    }

                    // ── Comments ──────────────────────────────────────────
                    Kind::Comment => {
                        semantic.add_node(&node, rope, TokenKind::Comment, 0u32);

                        let line = node.start_position().row;
                        match comment_start_line {
                            Some(_) => {
                                comment_end_line = line;
                            }
                            None => {
                                comment_start_line = Some(line);
                                comment_end_line = line;
                            }
                        }
                    }

                    _ => {}
                }
            }
        }

        // Flush remaining comment folding range.
        if let Some(start) = comment_start_line {
            if comment_end_line > start {
                folding.push(FoldingRange {
                    start_line: start,
                    end_line: comment_end_line,
                    kind: Some(FoldingRangeKind::Comment),
                    ..Default::default()
                });
            }
        }

        // Flush remaining symbols from the stack.
        while let Some((finished, _)) = symbol_stack.pop() {
            if let Some((parent, _)) = symbol_stack.last_mut() {
                parent.children.get_or_insert_with(Vec::new).push(finished);
            } else {
                symbols.push(finished);
            }
        }

        FOLDING_URI_MAP.insert(uri.clone(), folding);
        SYMBOL_URI_MAP.insert(uri.clone(), symbols);
        DIAGNOSTIC_URI_MAP.insert(uri.clone(), report);
        SEMANTIC_URI_MAP.insert(uri.clone(), semantic);

        uri_unlock(uri);
    }
    Ok(())
}
