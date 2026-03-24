//! IR pass: wrap array reads with type casts for AS `table` access.
//!
//! In JASS, `integer array foo` is a typed array.  In AS it becomes
//! `table foo = {};` — an untyped container.  Reading from a table
//! returns a generic `tableValue`; the AS compiler needs an explicit
//! cast to use it in a typed context: `int(foo[idx])`.
//!
//! This pass walks the IR and wraps every `IRExpr::Index` whose array
//! variable is known to be an array with an `IRExpr::Cast` that carries
//! the element type.  The cast is **only** applied to **reads** — array
//! writes (`set arr[i] = val`) are left alone because the LHS of an
//! assignment doesn't need casting.

use super::ir::*;
use std::collections::HashMap;

// ─── Array type collection ───────────────────────────────────────────────────

/// Build a map of `array_name → element_type` from global and local declarations.
fn collect_array_types(ir: &BuildIR) -> HashMap<String, String> {
    let mut map = HashMap::new();

    // Globals.
    for stmt in &ir.globals {
        if let IRStmt::VarDecl { is_array: true, type_name, decls, .. } = stmt {
            for d in decls {
                map.insert(d.name.clone(), type_name.clone());
            }
        }
    }

    // Locals inside function bodies (including late-declared ones).
    for func in ir.functions.values() {
        collect_array_locals(&func.body, &mut map);
    }

    map
}

fn collect_array_locals(stmts: &[IRStmt], map: &mut HashMap<String, String>) {
    for stmt in stmts {
        match stmt {
            IRStmt::Local { is_array: true, type_name, name, .. } => {
                map.insert(name.clone(), type_name.clone());
            }
            IRStmt::VarDecl { is_array: true, type_name, decls, .. } => {
                for d in decls {
                    map.insert(d.name.clone(), type_name.clone());
                }
            }
            IRStmt::If { body, branches, .. } => {
                collect_array_locals(body, map);
                for b in branches {
                    collect_array_locals(&b.body, map);
                }
            }
            IRStmt::Loop(body) => {
                collect_array_locals(body, map);
            }
            _ => {}
        }
    }
}

// ─── Expression wrapping ─────────────────────────────────────────────────────

/// Wrap `arr[idx]` reads with `Cast { type_name, inner }`.
fn wrap_expr(expr: &mut IRExpr, arrays: &HashMap<String, String>) {
    match expr {
        // Array read: wrap with Cast if the array name is known.
        // NOTE: this is only reached for reads — writes (Set LHS) are
        //       handled by the statement walker which skips the LHS.
        IRExpr::Index { array, index } => {
            // First recurse into sub-expressions.
            wrap_expr(array, arrays);
            wrap_expr(index, arrays);

            // If the array is a known typed array, wrap the whole Index.
            if let IRExpr::Id(name) = array.as_ref() {
                if let Some(elem_type) = arrays.get(name) {
                    let owned = std::mem::replace(
                        expr,
                        IRExpr::Literal("__placeholder".into()),
                    );
                    *expr = IRExpr::Cast {
                        type_name: elem_type.clone(),
                        inner: Box::new(owned),
                    };
                }
            }
        }

        IRExpr::Call { args, .. } => {
            for a in args { wrap_expr(a, arrays); }
        }
        IRExpr::Binary { left, right, .. } => {
            wrap_expr(left, arrays);
            wrap_expr(right, arrays);
        }
        IRExpr::Unary { operand, .. } => {
            wrap_expr(operand, arrays);
        }
        IRExpr::Parens(inner) => {
            wrap_expr(inner, arrays);
        }
        IRExpr::Cast { inner, .. } => {
            wrap_expr(inner, arrays);
        }
        IRExpr::Id(_) | IRExpr::Literal(_) | IRExpr::FuncRef(_) => {}
    }
}

// ─── Statement walking ───────────────────────────────────────────────────────

fn wrap_stmt(stmt: &mut IRStmt, arrays: &HashMap<String, String>) {
    match stmt {
        IRStmt::Local { value: Some(value), .. } => {
            wrap_expr(value, arrays);
        }
        IRStmt::Local { .. } => {}

        IRStmt::Set { var: _, index, value } => {
            // Wrap reads inside the index expression and the value.
            // Do NOT wrap the LHS `var[index]` itself — it's a write target.
            if let Some(idx) = index {
                wrap_expr(idx, arrays);
            }
            wrap_expr(value, arrays);
        }

        IRStmt::Call { args, .. } => {
            for a in args { wrap_expr(a, arrays); }
        }

        IRStmt::Return(Some(value)) => {
            wrap_expr(value, arrays);
        }
        IRStmt::Return(None) => {}

        IRStmt::Exitwhen(cond) => {
            wrap_expr(cond, arrays);
        }

        IRStmt::If { condition, body, branches } => {
            wrap_expr(condition, arrays);
            for s in body.iter_mut() { wrap_stmt(s, arrays); }
            for b in branches.iter_mut() {
                if let Some(ref mut cond) = b.condition { wrap_expr(cond, arrays); }
                for s in b.body.iter_mut() { wrap_stmt(s, arrays); }
            }
        }

        IRStmt::Loop(body) => {
            for s in body.iter_mut() { wrap_stmt(s, arrays); }
        }

        IRStmt::VarDecl { decls, .. } => {
            for d in decls.iter_mut() {
                if let Some(ref mut value) = d.value {
                    wrap_expr(value, arrays);
                }
            }
        }
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Wrap array reads with type casts for the AS build.
///
/// Call after `resolve_frozen_deps` and before rendering.
pub(super) fn insert_array_casts(ir: &mut BuildIR) {
    let arrays = collect_array_types(ir);
    if arrays.is_empty() { return; }

    // Globals.
    for stmt in &mut ir.globals {
        wrap_stmt(stmt, &arrays);
    }

    // Functions.
    for func in ir.functions.values_mut() {
        for stmt in &mut func.body {
            wrap_stmt(stmt, &arrays);
        }
    }
}

/// Wrap array reads in a single function (test pipeline).
#[cfg(test)]
pub(super) fn insert_array_casts_func(func: &mut IRFunc) {
    let mut arrays = HashMap::new();
    collect_array_locals(&func.body, &mut arrays);
    for stmt in &mut func.body {
        wrap_stmt(stmt, &arrays);
    }
}

