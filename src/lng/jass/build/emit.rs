//! CST-based emit functions.
//!
//! These functions work directly on the tree-sitter AST nodes to emit
//! properly formatted JASS (and AS-aware) source text.  Used primarily
//! by the inline-candidate detector and the test harness.

use crate::lng::jass::ast::{
    CallStmt, ExitwhenStmt, Expr, FunctionDecl, IfStmt, LocalDecl,
    ReturnStmt, SetStmt, Statement, VarStmt,
};
use crate::lng::jass::kind::Kind;
use super::convert::{binary_op_text, flatten, id_text};

/// Flatten an expression to a single-line string.
///
/// When `for_as` is `true`, the expression is recursively emitted with
/// parentheses inserted where JASS and AngelScript operator precedence
/// differs (specifically: `or` operands of `and` are wrapped).
#[allow(dead_code)]
pub(super) fn emit_expr(src: &str, expr: &Expr, for_as: bool) -> String {
    if for_as {
        return emit_expr_as(src, expr);
    }
    flatten(src, expr.cst_node())
}

/// Check whether an expression is a binary `or` expression.
#[allow(dead_code)]
fn is_or_expr(src: &str, expr: &Expr) -> bool {
    if let Expr::Binary { left, right, .. } = expr {
        binary_op_text(src, left, right) == "or"
    } else {
        false
    }
}

/// Recursively emit an expression for AngelScript output.
///
/// In JASS, `or` has **higher** precedence than `and` (binds tighter).
/// In AS / C++, `and` (`&&`) has higher precedence than `or` (`||`).
/// Therefore, when a child of `and` is an `or` expression, we wrap it in
/// parentheses so that AS interprets it with the same semantics as JASS.
fn emit_expr_as(src: &str, expr: &Expr) -> String {
    match expr {
        Expr::Binary { node: _, left, right } => {
            let op = binary_op_text(src, left, right);
            let left_str = if op == "and" && is_or_expr(src, left) {
                format!("({})", emit_expr_as(src, left))
            } else {
                emit_expr_as(src, left)
            };
            let right_str = if op == "and" && is_or_expr(src, right) {
                format!("({})", emit_expr_as(src, right))
            } else {
                emit_expr_as(src, right)
            };
            format!("{} {} {}", left_str, op, right_str)
        }
        Expr::Unary { node, operand } => {
            let op_end = operand.cst_node().start_byte();
            let op_text = src[node.start_byte()..op_end].trim();
            format!("{} {}", op_text, emit_expr_as(src, operand))
        }
        Expr::Parens { inner, .. } => {
            format!("({})", emit_expr_as(src, inner))
        }
        Expr::Call(fc) => {
            let name = fc.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
            let args: Vec<String> = fc.args.iter().map(|a| emit_expr_as(src, a)).collect();
            format!("{}({})", name, args.join(", "))
        }
        Expr::Index { array, index, .. } => {
            format!("{}[{}]", emit_expr_as(src, array), emit_expr_as(src, index))
        }
        Expr::FuncRef(id) => {
            format!("function {}", id_text(src, id))
        }
        Expr::Id(id) => id_text(src, id),
        Expr::Literal(node) => flatten(src, node),
    }
}

/// `set VAR[INDEX] = VALUE` — always emits the `set` keyword.
fn emit_set(src: &str, s: &SetStmt, for_as: bool) -> String {
    let var = s.variable.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
    let idx = match &s.index {
        Some(e) => format!("[{}]", emit_expr(src, e, for_as)),
        None => String::new(),
    };
    let val = s.value.as_ref().map(|e| emit_expr(src, e, for_as)).unwrap_or_default();
    format!("set {}{} = {}", var, idx, val)
}

/// `call FUNC(ARGS)` — always emits the `call` keyword.
#[allow(dead_code)]
fn emit_call(src: &str, c: &CallStmt, for_as: bool) -> String {
    match &c.func {
        Some(fc) => {
            let name = fc.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
            let args: Vec<String> = fc.args.iter().map(|a| emit_expr(src, a, for_as)).collect();
            format!("call {}({})", name, args.join(", "))
        }
        None => "call ???()".to_string(),
    }
}

/// `return [VALUE]`
#[allow(dead_code)]
fn emit_return(src: &str, r: &ReturnStmt, for_as: bool) -> String {
    match &r.value {
        Some(e) => format!("return {}", emit_expr(src, e, for_as)),
        None => "return".to_string(),
    }
}

/// `exitwhen COND`
#[allow(dead_code)]
fn emit_exitwhen(src: &str, e: &ExitwhenStmt, for_as: bool) -> String {
    let cond = e.condition.as_ref().map(|c| emit_expr(src, c, for_as)).unwrap_or_default();
    format!("exitwhen {}", cond)
}

/// `local TYPE NAME [= VALUE]`
#[allow(dead_code)]
fn emit_local(src: &str, l: &LocalDecl, for_as: bool) -> String {
    let type_name = l.type_id.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "integer".to_string());
    let name = l.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
    match &l.value {
        Some(e) => format!("local {} {} = {}", type_name, name, emit_expr(src, e, for_as)),
        None => format!("local {} {}", type_name, name),
    }
}

/// `[constant] TYPE [array] NAME [= VALUE], ...`
#[allow(dead_code)]
pub(super) fn emit_var(src: &str, v: &VarStmt, for_as: bool) -> String {
    let type_name = v.type_id.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "integer".to_string());
    let mut prefix = String::new();
    if v.is_constant { prefix.push_str("constant "); }
    prefix.push_str(&type_name);
    if v.is_array { prefix.push_str(" array"); }
    let decls: Vec<String> = v.decls.iter().map(|d| {
        let name = d.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
        match &d.value {
            Some(e) => format!("{} = {}", name, emit_expr(src, e, for_as)),
            None => name,
        }
    }).collect();
    format!("{} {}", prefix, decls.join(", "))
}

/// Emit a list of AST statements as properly formatted lines.
///
/// Each simple statement is one line; compound statements (`if`, `loop`)
/// expand into multiple lines with correct indentation.
#[allow(dead_code)]
fn emit_body(src: &str, stmts: &[Statement], indent: &str, for_as: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for stmt in stmts {
        match stmt {
            Statement::Set(s) => lines.push(format!("{}{}", indent, emit_set(src, s, for_as))),
            Statement::Call(c) => lines.push(format!("{}{}", indent, emit_call(src, c, for_as))),
            Statement::Return(r) => lines.push(format!("{}{}", indent, emit_return(src, r, for_as))),
            Statement::Exitwhen(e) => lines.push(format!("{}{}", indent, emit_exitwhen(src, e, for_as))),
            Statement::Local(l) => lines.push(format!("{}{}", indent, emit_local(src, l, for_as))),
            Statement::VarStmt(v) => lines.push(format!("{}local {}", indent, emit_var(src, v, for_as))),
            Statement::If(i) => lines.extend(emit_if(src, i, indent, for_as)),
            Statement::Loop(l) => {
                let inner = format!("{}    ", indent);
                lines.push(format!("{}loop", indent));
                lines.extend(emit_body(src, &l.body, &inner, for_as));
                lines.push(format!("{}endloop", indent));
            }
            _ => {}
        }
    }
    lines
}

/// Emit a single CST node as statement lines (used inside CST-based if/loop walkers).
#[allow(dead_code)]
fn emit_cst_node(src: &str, node: &tree_sitter::Node, kind: Kind, indent: &str, for_as: bool) -> Vec<String> {
    match kind {
        Kind::SetStatement | Kind::CallStatement | Kind::ReturnStatement
        | Kind::ExitwhenStatement | Kind::LocalStatement | Kind::VarStmt => {
            vec![format!("{}{}", indent, flatten(src, node))]
        }
        Kind::IfStatement => emit_if_cst(src, node, indent, for_as),
        Kind::LoopStatement => emit_loop_cst(src, node, indent, for_as),
        _ => vec![],
    }
}

/// Emit an `if`/`elseif`/`else`/`endif` block from the AST.
#[allow(dead_code)]
fn emit_if(src: &str, i: &IfStmt, indent: &str, for_as: bool) -> Vec<String> {
    let inner = format!("{}    ", indent);
    let mut lines = Vec::new();

    // First branch: `if COND then ...`
    let cond = i.condition.as_ref()
        .map(|c| emit_expr(src, c, for_as))
        .unwrap_or_default();
    lines.push(format!("{}if {} then", indent, cond));
    lines.extend(emit_body(src, &i.body, &inner, for_as));

    // Subsequent branches: `elseif COND then ...` / `else ...`
    for branch in &i.branches {
        if let Some(ref cond) = branch.condition {
            lines.push(format!("{}elseif {} then", indent, emit_expr(src, cond, for_as)));
        } else {
            lines.push(format!("{}else", indent));
        }
        lines.extend(emit_body(src, &branch.body, &inner, for_as));
    }

    lines.push(format!("{}endif", indent));
    lines
}

/// Walk a `loop_statement` CST node and emit properly formatted lines.
#[allow(dead_code)]
fn emit_loop_cst(src: &str, node: &tree_sitter::Node, indent: &str, for_as: bool) -> Vec<String> {
    let mut lines = Vec::new();
    let inner = format!("{}    ", indent);

    lines.push(format!("{}loop", indent));

    for idx in 0..node.child_count() as u32 {
        let child = match node.child(idx) {
            Some(c) => c,
            None => continue,
        };
        if !child.is_named() {
            continue;
        }
        if let Ok(nk) = Kind::try_from(child.kind_id()) {
            lines.extend(emit_cst_node(src, &child, nk, &inner, for_as));
        }
    }

    lines.push(format!("{}endloop", indent));
    lines
}

/// Walk an `if_statement` CST node and emit properly formatted lines.
///
/// CST structure (flat children):
///   `if` COND `then` STMTS [`elseif` COND `then` STMTS]* [`else` STMTS] `endif`
#[allow(dead_code)]
fn emit_if_cst(src: &str, node: &tree_sitter::Node, indent: &str, for_as: bool) -> Vec<String> {
    let inner = format!("{}    ", indent);
    let mut lines = Vec::new();

    // State machine phases matching the CST layout.
    enum Phase { IfCond, FirstBody, ElseifCond, ElseifBody, ElseBody }
    let mut phase = Phase::IfCond;
    let mut cond_parts: Vec<String> = Vec::new();

    for idx in 0..node.child_count() as u32 {
        let child = match node.child(idx) {
            Some(c) => c,
            None => continue,
        };

        let kind = Kind::try_from(child.kind_id()).ok();

        match (&phase, kind) {
            // `if` keyword — skip.
            (Phase::IfCond, Some(Kind::If)) => {}
            // Condition expression(s) before `then`.
            (Phase::IfCond, Some(Kind::Then)) => {
                let cond = cond_parts.join(" ");
                lines.push(format!("{}if {} then", indent, cond));
                cond_parts.clear();
                phase = Phase::FirstBody;
            }
            (Phase::IfCond, _) => {
                if child.is_named() {
                    cond_parts.push(flatten(src, &child));
                }
            }

            // First body — statements between `then` and `elseif`/`else`/`endif`.
            (Phase::FirstBody, Some(Kind::Elseif)) => {
                cond_parts.clear();
                phase = Phase::ElseifCond;
            }
            (Phase::FirstBody, Some(Kind::Else)) => {
                lines.push(format!("{}else", indent));
                phase = Phase::ElseBody;
            }
            (Phase::FirstBody, Some(Kind::Endif)) => {
                lines.push(format!("{}endif", indent));
            }
            (Phase::FirstBody, _) => {
                if child.is_named() {
                    if let Ok(nk) = Kind::try_from(child.kind_id()) {
                        lines.extend(emit_cst_node(src, &child, nk, &inner, for_as));
                    }
                }
            }

            // `elseif` condition.
            (Phase::ElseifCond, Some(Kind::Then)) => {
                let cond = cond_parts.join(" ");
                lines.push(format!("{}elseif {} then", indent, cond));
                cond_parts.clear();
                phase = Phase::ElseifBody;
            }
            (Phase::ElseifCond, _) => {
                if child.is_named() {
                    cond_parts.push(flatten(src, &child));
                }
            }

            // Elseif body.
            (Phase::ElseifBody, Some(Kind::Elseif)) => {
                cond_parts.clear();
                phase = Phase::ElseifCond;
            }
            (Phase::ElseifBody, Some(Kind::Else)) => {
                lines.push(format!("{}else", indent));
                phase = Phase::ElseBody;
            }
            (Phase::ElseifBody, Some(Kind::Endif)) => {
                lines.push(format!("{}endif", indent));
            }
            (Phase::ElseifBody, _) => {
                if child.is_named() {
                    if let Ok(nk) = Kind::try_from(child.kind_id()) {
                        lines.extend(emit_cst_node(src, &child, nk, &inner, for_as));
                    }
                }
            }

            // Else body.
            (Phase::ElseBody, Some(Kind::Endif)) => {
                lines.push(format!("{}endif", indent));
            }
            (Phase::ElseBody, _) => {
                if child.is_named() {
                    if let Ok(nk) = Kind::try_from(child.kind_id()) {
                        lines.extend(emit_cst_node(src, &child, nk, &inner, for_as));
                    }
                }
            }
        }
    }

    lines
}

/// Emit `function NAME takes PARAMS returns TYPE`
#[allow(dead_code)]
fn emit_func_sig(src: &str, f: &FunctionDecl) -> String {
    let name = f.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
    let params = if f.params.is_empty() {
        "nothing".to_string()
    } else {
        f.params
            .iter()
            .map(|p| {
                let t = p.type_id.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "integer".to_string());
                let n = p.name.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "_".to_string());
                format!("{} {}", t, n)
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let ret = f
        .return_type
        .as_ref()
        .map(|id| id_text(src, id))
        .unwrap_or_else(|| "nothing".to_string());
    format!("function {} takes {} returns {}", name, params, ret)
}

/// Emit a complete function: signature + indented body + endfunction.
#[allow(dead_code)]
pub(super) fn emit_function(src: &str, f: &FunctionDecl, for_as: bool) -> String {
    let sig = emit_func_sig(src, f);
    let body_lines = emit_body(src, &f.body, "    ", for_as);
    let mut out = sig;
    out.push('\n');
    for line in &body_lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("endfunction");
    out
}

/// Test-only wrapper for [`emit_function`].
#[cfg(test)]
pub fn emit_function_text(src: &str, f: &FunctionDecl) -> String {
    emit_function(src, f, false)
}

/// Test-only wrapper for [`emit_function`] in AS mode (with precedence fix).
#[cfg(test)]
pub fn emit_function_text_as(src: &str, f: &FunctionDecl) -> String {
    emit_function(src, f, true)
}

/// Test-only wrapper for [`emit_var`] in AS mode.
#[cfg(test)]
pub fn emit_var_text_as(src: &str, v: &VarStmt) -> String {
    emit_var(src, v, true)
}

