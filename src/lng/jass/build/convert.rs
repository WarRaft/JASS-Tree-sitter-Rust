//! AST → IR conversion.
//!
//! Converts tree-sitter AST nodes (`Expr`, `Statement`, `FunctionDecl`) into
//! the owned IR types (`IRExpr`, `IRStmt`, `IRFunc`), and collects the complete
//! [`BuildIR`] from all source files in the import tree.

use crate::lng::jass::ast::{
    build_ast, rewrite_imports, Expr, FunctionDecl, Id, Statement,
};
use crate::util::file_store::{is_uri_frozen, FILE_STORE};
use std::collections::{HashMap, HashSet};
use url::Url;

use super::inline::detect_inline_candidate;
use super::ir::*;
use super::io::read_file_source;

// ─── Text extraction helpers ─────────────────────────────────────────────────

/// Collapse a CST node's text to a single line (all whitespace → single space).
pub(super) fn flatten(src: &str, node: &tree_sitter::Node) -> String {
    let text = &src[node.start_byte()..node.end_byte()];
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Read raw identifier text.
pub(super) fn id_text(src: &str, id: &Id) -> String {
    src[id.node.start_byte()..id.node.end_byte()].to_string()
}

/// Extract the operator text from a binary expression by looking at the
/// source between the left and right operand CST spans.
pub(super) fn binary_op_text(src: &str, left: &Expr, right: &Expr) -> String {
    let left_end = left.cst_node().end_byte();
    let right_start = right.cst_node().start_byte();
    if left_end < right_start {
        src[left_end..right_start].trim().to_string()
    } else {
        String::new()
    }
}

// ─── AST → IR conversion ────────────────────────────────────────────────────

pub(super) fn convert_expr(src: &str, expr: &Expr) -> IRExpr {
    match expr {
        Expr::Id(id) => IRExpr::Id(id_text(src, id)),
        Expr::Literal(node) => IRExpr::Literal(flatten(src, node)),
        Expr::Call(fc) => {
            let name = fc.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
            let args = fc.args.iter().map(|a| convert_expr(src, a)).collect();
            IRExpr::Call { name, args }
        }
        Expr::FuncRef(id) => IRExpr::FuncRef(id_text(src, id)),
        Expr::Binary { left, right, .. } => {
            let op = binary_op_text(src, left, right);
            IRExpr::Binary {
                left: Box::new(convert_expr(src, left)),
                op,
                right: Box::new(convert_expr(src, right)),
            }
        }
        Expr::Unary { node, operand } => {
            let op_end = operand.cst_node().start_byte();
            let op = src[node.start_byte()..op_end].trim().to_string();
            IRExpr::Unary {
                op,
                operand: Box::new(convert_expr(src, operand)),
            }
        }
        Expr::Parens { inner, .. } => {
            IRExpr::Parens(Box::new(convert_expr(src, inner)))
        }
        Expr::Index { array, index, .. } => {
            IRExpr::Index {
                array: Box::new(convert_expr(src, array)),
                index: Box::new(convert_expr(src, index)),
            }
        }
    }
}

pub(super) fn convert_stmt(src: &str, stmt: &Statement) -> Option<IRStmt> {
    match stmt {
        Statement::Local(l) => {
            let type_name = l.type_id.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "integer".into());
            let name = l.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
            let value = l.value.as_ref().map(|e| convert_expr(src, e));
            Some(IRStmt::Local { type_name, is_array: l.is_array, name, value })
        }
        Statement::Set(s) => {
            let var = s.variable.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
            let index = s.index.as_ref().map(|e| convert_expr(src, e));
            let value = s.value.as_ref().map(|e| convert_expr(src, e)).unwrap_or(IRExpr::int(0));
            Some(IRStmt::Set { var, index, value })
        }
        Statement::Call(c) => {
            if let Some(fc) = &c.func {
                let name = fc.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
                let args = fc.args.iter().map(|a| convert_expr(src, a)).collect();
                Some(IRStmt::Call { name, args })
            } else {
                None
            }
        }
        Statement::Return(r) => {
            Some(IRStmt::Return(r.value.as_ref().map(|e| convert_expr(src, e))))
        }
        Statement::Exitwhen(e) => {
            Some(IRStmt::Exitwhen(
                e.condition.as_ref().map(|c| convert_expr(src, c)).unwrap_or(IRExpr::bool_val(true))
            ))
        }
        Statement::If(i) => {
            let condition = i.condition.as_ref()
                .map(|c| convert_expr(src, c))
                .unwrap_or(IRExpr::bool_val(true));
            let body = convert_body(src, &i.body);
            let branches = i.branches.iter().map(|b| IRBranch {
                condition: b.condition.as_ref().map(|c| convert_expr(src, c)),
                body: convert_body(src, &b.body),
            }).collect();
            Some(IRStmt::If { condition, body, branches })
        }
        Statement::Loop(l) => {
            Some(IRStmt::Loop(convert_body(src, &l.body)))
        }
        Statement::VarStmt(v) => {
            let type_name = v.type_id.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "integer".into());
            let decls = v.decls.iter().map(|d| IRVarInit {
                name: d.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default(),
                value: d.value.as_ref().map(|e| convert_expr(src, e)),
            }).collect();
            Some(IRStmt::VarDecl { is_constant: v.is_constant, is_array: v.is_array, type_name, decls })
        }
        _ => None,
    }
}

pub(super) fn convert_body(src: &str, stmts: &[Statement]) -> Vec<IRStmt> {
    stmts.iter().filter_map(|s| convert_stmt(src, s)).collect()
}

pub(super) fn convert_function(
    src: &str,
    f: &FunctionDecl,
    callees: HashSet<String>,
) -> IRFunc {
    let name = f.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
    let params: Vec<(String, String)> = f.params.iter().map(|p| {
        let t = p.type_id.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "integer".into());
        let n = p.name.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "_".into());
        (t, n)
    }).collect();
    let return_type = f.return_type.as_ref()
        .map(|id| id_text(src, id))
        .unwrap_or_else(|| "nothing".into());
    let body = convert_body(src, &f.body);

    // Detect inline candidate: takes nothing + single `return expr`.
    let inline_expr = if f.params.is_empty() {
        detect_inline_candidate(src, &f.body, false)
    } else {
        None
    };

    IRFunc { name, params, return_type, body, callees, inline_expr }
}

// ─── IR collection from source files ─────────────────────────────────────────

/// Collect the IR from all source files.
pub(super) fn collect_ir(_trigger_uri: &Url, file_order: &[Url]) -> BuildIR {
    let mut globals = Vec::<IRStmt>::new();
    let mut functions: HashMap<String, IRFunc> = HashMap::new();
    let mut bare_stmts = Vec::<IRStmt>::new();

    for file_uri in file_order {
        if is_uri_frozen(file_uri) { continue; }

        let src = match read_file_source(file_uri) {
            Some(s) => s,
            None => continue,
        };

        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&tree_sitter_jass::language().into()).is_err() { continue; }
        let tree = match parser.parse(&src, None) {
            Some(t) => t,
            None => continue,
        };

        let mut ast = build_ast(tree.root_node());
        let src_bytes = src.as_bytes().to_vec();
        rewrite_imports(&mut ast, &src_bytes);

        for item in &ast.items {
            match item {
                Statement::Type(_) | Statement::Native(_) => {}
                Statement::Globals(g) => {
                    for v in &g.vars {
                        if let Some(s) = convert_stmt(&src, &Statement::VarStmt(v.clone())) {
                            globals.push(s);
                        }
                    }
                }
                Statement::Function(f) => {
                    let fname = f.name.as_ref().map(|id| id_text(&src, id)).unwrap_or_default();
                    if !fname.is_empty() {
                        let callees: HashSet<String> = FILE_STORE
                            .get(file_uri)
                            .map(|fs| {
                                fs.file_symbols.functions.iter()
                                    .find(|ff| ff.name == fname)
                                    .map(|ff| ff.callees.clone())
                                    .unwrap_or_default()
                            })
                            .unwrap_or_default();
                        functions.insert(fname.clone(), convert_function(&src, f, callees));
                    }
                }
                Statement::VarStmt(v) => {
                    if let Some(s) = convert_stmt(&src, &Statement::VarStmt(v.clone())) {
                        globals.push(s);
                    }
                }
                Statement::Set(_) | Statement::Call(_) | Statement::If(_) | Statement::Loop(_) => {
                    if let Some(s) = convert_stmt(&src, item) {
                        bare_stmts.push(s);
                    }
                }
                _ => {}
            }
        }
    }

    BuildIR { globals, functions, bare_stmts }
}

