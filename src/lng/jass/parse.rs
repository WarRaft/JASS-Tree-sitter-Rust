use std::collections::HashMap;
use crate::lng::jass::ast::{self, Ast, IdRole, Statement, build_ast};
use crate::lng::jass::kind::Kind;
use crate::lng::jass::uri_map::TREE_MAP;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity, DocumentDiagnosticReport};
use crate::lsp::diagnostic::uri_map::URI_MAP as DIAGNOSTIC_URI_MAP;
use crate::lsp::document_symbol::lsp::{DocumentSymbol, SymbolKind};
use crate::lsp::document_symbol::uri_map::URI_MAP as SYMBOL_URI_MAP;
use crate::lsp::folding::lsp::{FoldingRange, FoldingRangeKind};
use crate::lsp::folding::uri_map::URI_MAP as FOLDING_URI_MAP;
use crate::lsp::position::Position;
use crate::lsp::range::Range;
use crate::lsp::semantic::hub::Hub;
use crate::lsp::semantic::lsp::Kind as TokenKind;
use crate::lsp::semantic::uri_map::URI_MAP as SEMANTIC_URI_MAP;
use crate::util::dfs_node::Dfs;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_lock::uri_unlock;
use lapce_xi_rope::Rope;
use std::error::Error;
use url::Url;

// ─── Span → LSP Range ───────────────────────────────────────────────────────

fn span_to_range(span: &ast::Span, rope: &Rope) -> Range {
    Range {
        start: Position::from_byte_offset(rope, span.start_byte).unwrap_or_default(),
        end: Position::from_byte_offset(rope, span.end_byte).unwrap_or_default(),
    }
}

fn id_sel_range(id: &Option<ast::Id>, fallback: &ast::Span, rope: &Rope) -> Range {
    id.as_ref()
        .map(|id| span_to_range(&id.span, rope))
        .unwrap_or_else(|| span_to_range(fallback, rope))
}

fn id_name(id: &Option<ast::Id>, rope: &Rope) -> String {
    id.as_ref()
        .map(|id| rope.slice_to_cow(id.span.start_byte..id.span.end_byte).to_string())
        .unwrap_or_else(|| "<unnamed>".into())
}

// ─── AST → Diagnostics ──────────────────────────────────────────────────────

fn build_diagnostics(ast: &Ast, rope: &Rope) -> Vec<Diagnostic> {
    ast.errors
        .iter()
        .map(|e| Diagnostic {
            range: span_to_range(&e.span, rope),
            message: e.message.clone(),
            severity: Some(DiagnosticSeverity::Error),
            ..Default::default()
        })
        .collect()
}

// ─── AST → Document Symbols ─────────────────────────────────────────────────

fn build_symbols(ast: &Ast, rope: &Rope) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    for stmt in &ast.items {
        if let Some(sym) = stmt_to_symbol(stmt, rope) {
            symbols.push(sym);
        }
    }
    symbols
}

fn stmt_to_symbol(stmt: &Statement, rope: &Rope) -> Option<DocumentSymbol> {
    match stmt {
        Statement::Type(t) => Some(DocumentSymbol {
            name: id_name(&t.name, rope),
            kind: SymbolKind::Class,
            range: span_to_range(&t.span, rope),
            selection_range: id_sel_range(&t.name, &t.span, rope),
            ..Default::default()
        }),
        Statement::Native(n) => Some(DocumentSymbol {
            name: id_name(&n.name, rope),
            kind: SymbolKind::Interface,
            range: span_to_range(&n.span, rope),
            selection_range: id_sel_range(&n.name, &n.span, rope),
            ..Default::default()
        }),
        Statement::Function(f) => {
            let mut children = Vec::new();
            // Parameters as children
            for p in &f.params {
                children.push(DocumentSymbol {
                    name: id_name(&p.name, rope),
                    detail: p.type_id.as_ref().map(|id| {
                        rope.slice_to_cow(id.span.start_byte..id.span.end_byte).to_string()
                    }),
                    kind: SymbolKind::Variable,
                    range: span_to_range(&p.span, rope),
                    selection_range: id_sel_range(&p.name, &p.span, rope),
                    ..Default::default()
                });
            }
            // Body statements as children
            for s in &f.body {
                if let Some(sym) = stmt_to_symbol(s, rope) {
                    children.push(sym);
                }
            }
            Some(DocumentSymbol {
                name: id_name(&f.name, rope),
                kind: SymbolKind::Function,
                range: span_to_range(&f.span, rope),
                selection_range: id_sel_range(&f.name, &f.span, rope),
                children: if children.is_empty() { None } else { Some(children) },
                ..Default::default()
            })
        }
        Statement::Globals(g) => {
            let mut children = Vec::new();
            for v in &g.vars {
                for d in &v.decls {
                    children.push(DocumentSymbol {
                        name: id_name(&d.name, rope),
                        detail: v.type_id.as_ref().map(|id| {
                            rope.slice_to_cow(id.span.start_byte..id.span.end_byte).to_string()
                        }),
                        kind: if v.is_constant { SymbolKind::Constant } else { SymbolKind::Variable },
                        range: span_to_range(&d.span, rope),
                        selection_range: id_sel_range(&d.name, &d.span, rope),
                        ..Default::default()
                    });
                }
            }
            Some(DocumentSymbol {
                name: "globals".into(),
                kind: SymbolKind::Namespace,
                range: span_to_range(&g.span, rope),
                selection_range: span_to_range(&g.span, rope),
                children: if children.is_empty() { None } else { Some(children) },
                ..Default::default()
            })
        }
        Statement::Local(l) => Some(DocumentSymbol {
            name: id_name(&l.name, rope),
            detail: l.type_id.as_ref().map(|id| {
                rope.slice_to_cow(id.span.start_byte..id.span.end_byte).to_string()
            }),
            kind: SymbolKind::Variable,
            range: span_to_range(&l.span, rope),
            selection_range: id_sel_range(&l.name, &l.span, rope),
            ..Default::default()
        }),
        Statement::VarStmt(v) => {
            // Top-level var_stmt (outside globals) — show each decl
            for d in &v.decls {
                return Some(DocumentSymbol {
                    name: id_name(&d.name, rope),
                    kind: if v.is_constant { SymbolKind::Constant } else { SymbolKind::Variable },
                    range: span_to_range(&d.span, rope),
                    selection_range: id_sel_range(&d.name, &d.span, rope),
                    ..Default::default()
                });
            }
            None
        }
        _ => None,
    }
}

// ─── AST → Folding Ranges ───────────────────────────────────────────────────

fn build_folding(ast: &Ast) -> Vec<FoldingRange> {
    let mut folding = Vec::new();
    collect_folding(&ast.items, &mut folding);
    // Comment folding: consecutive comment lines
    collect_comment_folding(&ast.items, &mut folding);
    folding
}

fn collect_folding(stmts: &[Statement], folding: &mut Vec<FoldingRange>) {
    for stmt in stmts {
        match stmt {
            Statement::Function(f) => {
                if f.span.end_row > f.span.start_row {
                    folding.push(FoldingRange {
                        start_line: f.span.start_row,
                        end_line: f.span.end_row,
                        kind: Some(FoldingRangeKind::Region),
                        ..Default::default()
                    });
                }
                collect_folding(&f.body, folding);
            }
            Statement::Globals(g) => {
                if g.span.end_row > g.span.start_row {
                    folding.push(FoldingRange {
                        start_line: g.span.start_row,
                        end_line: g.span.end_row,
                        kind: Some(FoldingRangeKind::Region),
                        ..Default::default()
                    });
                }
            }
            Statement::If(i) => {
                if i.span.end_row > i.span.start_row {
                    folding.push(FoldingRange {
                        start_line: i.span.start_row,
                        end_line: i.span.end_row,
                        kind: Some(FoldingRangeKind::Region),
                        ..Default::default()
                    });
                }
                collect_folding(&i.body, folding);
            }
            Statement::Loop(l) => {
                if l.span.end_row > l.span.start_row {
                    folding.push(FoldingRange {
                        start_line: l.span.start_row,
                        end_line: l.span.end_row,
                        kind: Some(FoldingRangeKind::Region),
                        ..Default::default()
                    });
                }
                collect_folding(&l.body, folding);
            }
            _ => {}
        }
    }
}

fn collect_comment_folding(stmts: &[Statement], folding: &mut Vec<FoldingRange>) {
    let mut start: Option<usize> = None;
    let mut end: usize = 0;

    for stmt in stmts {
        match stmt {
            Statement::Comment(c) => {
                let row = c.span.start_row;
                match start {
                    Some(_) => end = row,
                    None => {
                        start = Some(row);
                        end = row;
                    }
                }
            }
            _ => {
                if let Some(s) = start.take() {
                    if end > s {
                        folding.push(FoldingRange {
                            start_line: s,
                            end_line: end,
                            kind: Some(FoldingRangeKind::Comment),
                            ..Default::default()
                        });
                    }
                }
                // Recurse into nested bodies for comment folding
                match stmt {
                    Statement::Function(f) => collect_comment_folding(&f.body, folding),
                    Statement::If(i) => collect_comment_folding(&i.body, folding),
                    Statement::Loop(l) => collect_comment_folding(&l.body, folding),
                    _ => {}
                }
            }
        }
    }
    // Flush trailing
    if let Some(s) = start {
        if end > s {
            folding.push(FoldingRange {
                start_line: s,
                end_line: end,
                kind: Some(FoldingRangeKind::Comment),
                ..Default::default()
            });
        }
    }
}

// ─── AST + CST → Semantic Tokens ────────────────────────────────────────────

fn id_role_to_token_kind(role: IdRole) -> TokenKind {
    match role {
        IdRole::FunctionDecl | IdRole::FunctionRef => TokenKind::Function,
        IdRole::TypeDecl | IdRole::TypeRef => TokenKind::Type,
        IdRole::Param => TokenKind::Parameter,
        IdRole::Variable => TokenKind::Variable,
        IdRole::Constant => TokenKind::Variable, // modifiers would distinguish
    }
}

fn build_semantic(ast: &Ast, root: tree_sitter::Node, rope: &Rope) -> Hub {
    // Build lookup: start_byte → IdRole  (from AST)
    let ids = ast.collect_ids();
    let id_roles: HashMap<usize, IdRole> = ids
        .into_iter()
        .map(|id| (id.span.start_byte, id.role))
        .collect();

    let mut semantic = Hub::default();

    // DFS over CST for all terminal tokens
    for node in Dfs::new(root) {
        if node.child_count() > 0 {
            // Only process leaf nodes for semantic tokens
            continue;
        }

        let Ok(kind) = Kind::try_from(node.grammar_id()) else {
            continue;
        };

        let token_kind = match kind {
            // Identifiers: look up role from AST
            Kind::IdToken => {
                if let Some(&role) = id_roles.get(&node.start_byte()) {
                    id_role_to_token_kind(role)
                } else {
                    TokenKind::Variable // fallback for ids not in AST (e.g. in expressions)
                }
            }

            // Keywords
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
            | Kind::Not => TokenKind::Keyword,

            // Operators
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
            | Kind::Neq => TokenKind::Operator,

            // Literals
            Kind::Number | Kind::Float | Kind::Rawcode => TokenKind::Number,
            Kind::StringContent | Kind::Quote => TokenKind::String,

            // Comments
            Kind::Comment => TokenKind::Comment,

            _ => continue,
        };

        semantic.add_node(&node, rope, token_kind, 0u32);
    }

    semantic
}

// ─── Main parse entry point ─────────────────────────────────────────────────

pub async fn parse(uri: &Url) -> Result<(), Box<dyn Error + Send + Sync>> {
    {
        let rope_entry = ROPE_MAP.get(&uri.clone()).ok_or("no rope")?;
        let rope: &Rope = rope_entry.value();

        let tree_entry = TREE_MAP.get(&uri.clone()).ok_or("no tree")?;
        let root = tree_entry.value().root_node();

        // 1. Build AST from CST
        let ast = build_ast(root);

        // 2. Diagnostics from AST errors
        let diagnostics = build_diagnostics(&ast, rope);
        let report = DocumentDiagnosticReport::Full {
            result_id: None,
            items: diagnostics,
            related_documents: None,
        };

        // 3. Document symbols from AST
        let symbols = build_symbols(&ast, rope);

        // 4. Folding ranges from AST
        let folding = build_folding(&ast);

        // 5. Semantic tokens from AST + CST
        let semantic = build_semantic(&ast, root, rope);

        FOLDING_URI_MAP.insert(uri.clone(), folding);
        SYMBOL_URI_MAP.insert(uri.clone(), symbols);
        DIAGNOSTIC_URI_MAP.insert(uri.clone(), report);
        SEMANTIC_URI_MAP.insert(uri.clone(), semantic);

        uri_unlock(uri);
    }
    Ok(())
}
