//! IR-level folding of `StringHash(…)` and `ExecuteFunc(…)`.
//!
//! Walks the owned IR tree and:
//! - **`StringHash(expr)`** — evaluates the argument at compile time; if it
//!   resolves to a string, replaces the call with the precomputed integer hash.
//! - **`ExecuteFunc(expr)`** — evaluates the argument at compile time; if it
//!   resolves to a string, replaces `ExecuteFunc("Some" + "Func")` with a
//!   direct `call SomeFunc()`.
//!
//! Constant evaluation rules:
//! - String / integer literals → constant.
//! - Identifiers → constant **only** if declared as `constant` (in the map).
//! - Binary `+` → string concatenation or integer addition.
//! - Other binary ops (`-`, `*`, `/`) → integer only.
//! - Unary `-` → integer only.
//! - `(expr)` → evaluate inner.
//! - **Any function call** → non-computable.
//! - Everything else (array index, func ref, cast) → non-computable.

use std::collections::HashMap;

use crate::util::string_hash::blizzard_string_hash;
use super::ir::*;

// ─── Compile-time constant value ─────────────────────────────────────────────

/// A value that was successfully evaluated at compile time from the IR.
#[derive(Debug, Clone)]
enum IRConst {
    Str(String),
    Int(i32),
}

// ─── Collect constants from IR globals ───────────────────────────────────────

/// Build a map of compile-time constants from `constant` global declarations.
///
/// Constants are processed in order so that later entries can reference earlier
/// ones (e.g. `constant string B = A + " world"`).
fn collect_ir_constants(globals: &[IRStmt]) -> HashMap<String, IRConst> {
    let mut map = HashMap::new();
    for g in globals {
        if let IRStmt::VarDecl { is_constant: true, is_array: false, type_name, decls } = g {
            for d in decls {
                if let Some(ref expr) = d.value {
                    if let Some(val) = eval_ir_expr(expr, &map) {
                        match (&val, type_name.as_str()) {
                            (IRConst::Str(_), "string")
                            | (IRConst::Int(_), "integer") => {
                                map.insert(d.name.clone(), val);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    map
}

// ─── IR expression evaluator ─────────────────────────────────────────────────

/// Try to evaluate an [`IRExpr`] to a compile-time constant.
///
/// Returns `None` when the expression cannot be fully resolved (contains a
/// non-constant variable, a function call, an array access, etc.).
fn eval_ir_expr(expr: &IRExpr, constants: &HashMap<String, IRConst>) -> Option<IRConst> {
    match expr {
        IRExpr::Literal(s) => parse_literal(s),
        IRExpr::Id(name) => constants.get(name).cloned(),
        IRExpr::Binary { left, op, right } => {
            let l = eval_ir_expr(left, constants)?;
            let r = eval_ir_expr(right, constants)?;
            eval_binary(l, op, r)
        }
        IRExpr::Unary { op, operand } => {
            let v = eval_ir_expr(operand, constants)?;
            match (op.as_str(), v) {
                ("-", IRConst::Int(n)) => Some(IRConst::Int(n.wrapping_neg())),
                _ => None,
            }
        }
        IRExpr::Parens(inner) => eval_ir_expr(inner, constants),
        // Any function call → non-computable.
        IRExpr::Call { .. } => None,
        // Array index, func ref, cast → non-computable.
        _ => None,
    }
}

/// Parse a literal string from the IR into an [`IRConst`].
fn parse_literal(s: &str) -> Option<IRConst> {
    // String literal: "…"
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        return Some(IRConst::Str(unescape_jass(inner)));
    }
    // FourCC: 'ABCD'
    if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        let mut val: i32 = 0;
        for b in inner.bytes() {
            val = (val << 8) | (b as i32);
        }
        return Some(IRConst::Int(val));
    }
    // Hex: 0x… / 0X…
    if s.starts_with("0x") || s.starts_with("0X") {
        return i32::from_str_radix(&s[2..], 16).ok().map(IRConst::Int);
    }
    // JASS hex: $…
    if s.starts_with('$') && s.len() > 1 {
        return i32::from_str_radix(&s[1..], 16).ok().map(IRConst::Int);
    }
    // Decimal integer
    if let Ok(n) = s.parse::<i32>() {
        return Some(IRConst::Int(n));
    }
    // `true` / `false` / `null` etc. — not usable for string hash.
    None
}

/// Unescape a JASS string body (the content between the quotes).
fn unescape_jass(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'\\' => out.push('\\'),
                b'"'  => out.push('"'),
                b'n'  => out.push('\n'),
                b'r'  => out.push('\r'),
                b't'  => out.push('\t'),
                other => { out.push('\\'); out.push(other as char); }
            }
            i += 2;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Evaluate a binary operation on two constants.
fn eval_binary(l: IRConst, op: &str, r: IRConst) -> Option<IRConst> {
    match op {
        "+" => match (l, r) {
            (IRConst::Str(a), IRConst::Str(b)) => Some(IRConst::Str(a + &b)),
            (IRConst::Int(a), IRConst::Int(b)) => Some(IRConst::Int(a.wrapping_add(b))),
            (IRConst::Str(a), IRConst::Int(b)) => Some(IRConst::Str(format!("{}{}", a, b))),
            (IRConst::Int(a), IRConst::Str(b)) => Some(IRConst::Str(format!("{}{}", a, b))),
        },
        "-" => match (l, r) {
            (IRConst::Int(a), IRConst::Int(b)) => Some(IRConst::Int(a.wrapping_sub(b))),
            _ => None,
        },
        "*" => match (l, r) {
            (IRConst::Int(a), IRConst::Int(b)) => Some(IRConst::Int(a.wrapping_mul(b))),
            _ => None,
        },
        "/" => match (l, r) {
            (IRConst::Int(a), IRConst::Int(b)) if b != 0 => Some(IRConst::Int(a.wrapping_div(b))),
            _ => None,
        },
        _ => None,
    }
}

// ─── Expression folding ──────────────────────────────────────────────────────

/// Recursively fold `StringHash(…)` calls inside an expression.
///
/// When a `StringHash(expr)` with a single argument is encountered and the
/// argument evaluates to a string constant, the entire call is replaced with
/// the precomputed integer literal.
fn fold_expr(expr: &mut IRExpr, constants: &HashMap<String, IRConst>) {
    match expr {
        IRExpr::Call { name, args } if name == "StringHash" && args.len() == 1 => {
            // Try to evaluate the argument.
            if let Some(IRConst::Str(s)) = eval_ir_expr(&args[0], constants) {
                let hash = blizzard_string_hash(&s);
                *expr = IRExpr::Literal(hash.to_string());
                return;
            }
            // Could not fold — still recurse into sub-expressions.
            for arg in args.iter_mut() {
                fold_expr(arg, constants);
            }
        }
        IRExpr::Call { args, .. } => {
            for arg in args.iter_mut() {
                fold_expr(arg, constants);
            }
        }
        IRExpr::Binary { left, right, .. } => {
            fold_expr(left, constants);
            fold_expr(right, constants);
        }
        IRExpr::Unary { operand, .. } => {
            fold_expr(operand, constants);
        }
        IRExpr::Parens(inner) => {
            fold_expr(inner, constants);
        }
        IRExpr::Index { array, index } => {
            fold_expr(array, constants);
            fold_expr(index, constants);
        }
        IRExpr::Cast { inner, .. } => {
            fold_expr(inner, constants);
        }
        // Literals, Ids, FuncRefs — nothing to fold.
        _ => {}
    }
}

// ─── Statement folding ───────────────────────────────────────────────────────

/// Fold `StringHash(…)` and `ExecuteFunc(…)` inside a statement.
///
/// - `StringHash(…)` is folded in every expression position.
/// - `ExecuteFunc(expr)` at statement level: if the argument evaluates to a
///   string, the call is rewritten to a direct `call <name>()`.
fn fold_stmt(stmt: &mut IRStmt, constants: &HashMap<String, IRConst>) {
    match stmt {
        IRStmt::Call { name, args } => {
            // ExecuteFunc("SomeFunc") → call SomeFunc()
            if name == "ExecuteFunc" && args.len() == 1 {
                if let Some(IRConst::Str(s)) = eval_ir_expr(&args[0], constants) {
                    *name = s;
                    args.clear();
                    return;
                }
            }
            for arg in args.iter_mut() {
                fold_expr(arg, constants);
            }
        }
        IRStmt::Local { value, .. } => {
            if let Some(v) = value {
                fold_expr(v, constants);
            }
        }
        IRStmt::Set { index, value, .. } => {
            if let Some(idx) = index {
                fold_expr(idx, constants);
            }
            fold_expr(value, constants);
        }
        IRStmt::Return(Some(v)) => {
            fold_expr(v, constants);
        }
        IRStmt::Return(None) => {}
        IRStmt::Exitwhen(cond) => {
            fold_expr(cond, constants);
        }
        IRStmt::If { condition, body, branches } => {
            fold_expr(condition, constants);
            for s in body.iter_mut() {
                fold_stmt(s, constants);
            }
            for b in branches.iter_mut() {
                if let Some(ref mut cond) = b.condition {
                    fold_expr(cond, constants);
                }
                for s in b.body.iter_mut() {
                    fold_stmt(s, constants);
                }
            }
        }
        IRStmt::Loop(body) => {
            for s in body.iter_mut() {
                fold_stmt(s, constants);
            }
        }
        IRStmt::VarDecl { decls, .. } => {
            for d in decls.iter_mut() {
                if let Some(ref mut v) = d.value {
                    fold_expr(v, constants);
                }
            }
        }

        IRStmt::TargetOnly { inner, .. } => {
            fold_stmt(inner, constants);
        }
    }
}

// ─── Public entry point ──────────────────────────────────────────────────────

/// Fold `StringHash(…)` → integer and `ExecuteFunc(…)` → direct call across
/// the entire [`BuildIR`].
///
/// Also updates function `callees` sets when `ExecuteFunc` is resolved to a
/// direct call.
pub(super) fn fold_ir(ir: &mut BuildIR) {
    let constants = collect_ir_constants(&ir.globals);

    // Fold in globals.
    for g in ir.globals.iter_mut() {
        fold_stmt(g, &constants);
    }

    // Fold in function bodies and update callees.
    let func_names: Vec<String> = ir.functions.keys().cloned().collect();
    for fname in &func_names {
        if let Some(func) = ir.functions.get_mut(fname) {
            // Collect ExecuteFunc targets before folding so we can update callees.
            let old_has_execute_func = func.callees.contains("ExecuteFunc");

            for stmt in func.body.iter_mut() {
                fold_stmt(stmt, &constants);
            }

            // Re-scan for new direct call targets introduced by ExecuteFunc folding.
            if old_has_execute_func {
                update_callees_after_fold(func);
            }
        }
    }

    // Fold in bare statements.
    for stmt in ir.bare_stmts.iter_mut() {
        fold_stmt(stmt, &constants);
    }
}

/// Re-scan a function's body to rebuild the callees set.
///
/// Called after folding to pick up new direct call targets that replaced
/// `ExecuteFunc(…)` and to drop `ExecuteFunc` if no unfolded calls remain.
fn update_callees_after_fold(func: &mut IRFunc) {
    func.callees.clear();
    for stmt in &func.body {
        collect_callees_stmt(stmt, &mut func.callees);
    }
}

fn collect_callees_stmt(stmt: &IRStmt, callees: &mut std::collections::HashSet<String>) {
    match stmt {
        IRStmt::Call { name, args } => {
            callees.insert(name.clone());
            for arg in args {
                collect_callees_expr(arg, callees);
            }
        }
        IRStmt::Local { value: Some(v), .. } => collect_callees_expr(v, callees),
        IRStmt::Set { index, value, .. } => {
            if let Some(idx) = index { collect_callees_expr(idx, callees); }
            collect_callees_expr(value, callees);
        }
        IRStmt::Return(Some(v)) => collect_callees_expr(v, callees),
        IRStmt::Exitwhen(cond) => collect_callees_expr(cond, callees),
        IRStmt::If { condition, body, branches } => {
            collect_callees_expr(condition, callees);
            for s in body { collect_callees_stmt(s, callees); }
            for b in branches {
                if let Some(ref cond) = b.condition { collect_callees_expr(cond, callees); }
                for s in &b.body { collect_callees_stmt(s, callees); }
            }
        }
        IRStmt::Loop(body) => {
            for s in body { collect_callees_stmt(s, callees); }
        }
        IRStmt::VarDecl { decls, .. } => {
            for d in decls {
                if let Some(ref v) = d.value { collect_callees_expr(v, callees); }
            }
        }
        _ => {}
    }
}

fn collect_callees_expr(expr: &IRExpr, callees: &mut std::collections::HashSet<String>) {
    match expr {
        IRExpr::Call { name, args } => {
            callees.insert(name.clone());
            for arg in args { collect_callees_expr(arg, callees); }
        }
        IRExpr::Binary { left, right, .. } => {
            collect_callees_expr(left, callees);
            collect_callees_expr(right, callees);
        }
        IRExpr::Unary { operand, .. } => collect_callees_expr(operand, callees),
        IRExpr::Parens(inner) => collect_callees_expr(inner, callees),
        IRExpr::Index { array, index } => {
            collect_callees_expr(array, callees);
            collect_callees_expr(index, callees);
        }
        IRExpr::Cast { inner, .. } => collect_callees_expr(inner, callees),
        _ => {}
    }
}

