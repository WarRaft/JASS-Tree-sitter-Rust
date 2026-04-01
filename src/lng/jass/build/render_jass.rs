//! IR → JASS rendering.
//!
//! Converts IR nodes back into JASS source text.  Also contains the
//! local-hoisting pass that ensures `local` declarations appear before
//! any executable statement (a JASS requirement).

use super::ir::*;
use super::render_as::jass_type_to_as_type;
use std::collections::{HashMap, HashSet};

// Re-import target lang bitmask.
use super::ir::TARGET_JASS;

// ─── IR → JASS rendering ────────────────────────────────────────────────────

pub(super) fn render_jass_expr(expr: &IRExpr, rn: &HashMap<String, String>) -> String {
    match expr {
        IRExpr::Literal(s) => s.clone(),
        IRExpr::Id(s) => ir_rename(s, rn),
        IRExpr::Call { name, args } => {
            let n = ir_rename(name, rn);
            let a: Vec<String> = args.iter().map(|a| render_jass_expr(a, rn)).collect();
            format!("{}({})", n, a.join(", "))
        }
        IRExpr::FuncRef(s) => format!("function {}", ir_rename(s, rn)),
        IRExpr::Binary { left, op, right } => {
            let left_str = if op == "and" && matches!(left.as_ref(), IRExpr::Binary { op: o, .. } if o == "or") {
                format!("({})", render_jass_expr(left, rn))
            } else {
                render_jass_expr(left, rn)
            };
            let right_str = if op == "and" && matches!(right.as_ref(), IRExpr::Binary { op: o, .. } if o == "or") {
                format!("({})", render_jass_expr(right, rn))
            } else {
                render_jass_expr(right, rn)
            };
            format!("{} {} {}", left_str, op, right_str)
        }
        IRExpr::Unary { op, operand } => {
            format!("{} {}", op, render_jass_expr(operand, rn))
        }
        IRExpr::Parens(inner) => format!("({})", render_jass_expr(inner, rn)),
        IRExpr::Index { array, index } => {
            format!("{}[{}]", render_jass_expr(array, rn), render_jass_expr(index, rn))
        }
        IRExpr::Cast { type_name, inner } => {
            format!("{}({})", jass_type_to_as_type(type_name), render_jass_expr(inner, rn))
        }
    }
}

pub(super) fn render_jass_stmt(stmt: &IRStmt, indent: &str, rn: &HashMap<String, String>) -> Vec<String> {
    match stmt {
        IRStmt::Local { type_name, is_array, name, short_name, value } => {
            let arr = if *is_array { " array" } else { "" };
            let dn = decl_name(name, short_name);
            match value {
                Some(v) if !*is_array => vec![format!("{}local {}{} {} = {}", indent, type_name, arr, dn, render_jass_expr(v, rn))],
                _ => vec![format!("{}local {}{} {}", indent, type_name, arr, dn)],
            }
        }
        IRStmt::Set { var, index, value } => {
            let v = ir_rename(var, rn);
            let idx = index.as_ref().map(|i| format!("[{}]", render_jass_expr(i, rn))).unwrap_or_default();
            vec![format!("{}set {}{} = {}", indent, v, idx, render_jass_expr(value, rn))]
        }
        IRStmt::Call { name, args } => {
            let n = ir_rename(name, rn);
            let a: Vec<String> = args.iter().map(|a| render_jass_expr(a, rn)).collect();
            vec![format!("{}call {}({})", indent, n, a.join(", "))]
        }
        IRStmt::Return(value) => {
            match value {
                Some(v) => vec![format!("{}return {}", indent, render_jass_expr(v, rn))],
                None => vec![format!("{}return", indent)],
            }
        }
        IRStmt::Exitwhen(cond) => {
            vec![format!("{}exitwhen {}", indent, render_jass_expr(cond, rn))]
        }
        IRStmt::If { condition, body, branches } => {
            let inner = format!("{}    ", indent);
            let mut lines = vec![format!("{}if {} then", indent, render_jass_expr(condition, rn))];
            for s in body { lines.extend(render_jass_stmt(s, &inner, rn)); }
            for b in branches {
                if let Some(ref cond) = b.condition {
                    lines.push(format!("{}elseif {} then", indent, render_jass_expr(cond, rn)));
                } else {
                    lines.push(format!("{}else", indent));
                }
                for s in &b.body { lines.extend(render_jass_stmt(s, &inner, rn)); }
            }
            lines.push(format!("{}endif", indent));
            lines
        }
        IRStmt::Loop(body) => {
            let inner = format!("{}    ", indent);
            let mut lines = vec![format!("{}loop", indent)];
            for s in body { lines.extend(render_jass_stmt(s, &inner, rn)); }
            lines.push(format!("{}endloop", indent));
            lines
        }
        IRStmt::VarDecl { is_constant, is_array, type_name, decls } => {
            let mut prefix = String::new();
            if *is_constant { prefix.push_str("constant "); }
            prefix.push_str(type_name);
            if *is_array { prefix.push_str(" array"); }
            let d: Vec<String> = decls.iter().map(|d| {
                let dn = decl_name(&d.name, &d.short_name);
                match &d.value {
                    Some(v) if !*is_array => format!("{} = {}", dn, render_jass_expr(v, rn)),
                    _ => dn,
                }
            }).collect();
            vec![format!("{}{} {}", indent, prefix, d.join(", "))]
        }
        IRStmt::TargetOnly { target, inner } => {
            if *target & TARGET_JASS != 0 {
                render_jass_stmt(inner, indent, rn)
            } else {
                vec![]
            }
        }
    }
}

/// Build a per-function scope rename map from the function's declarations.
///
/// The map is seeded with `global_map` (function names, global vars).
/// Then params are added in order (last wins for same-name shadowing),
/// then top-level locals (override params).
pub(super) fn build_func_scope_map(func: &IRFunc, global_map: &HashMap<String, String>) -> HashMap<String, String> {
    let mut map = global_map.clone();
    // Params (last wins for same-name).
    for p in &func.params {
        if let Some(ref sn) = p.short_name {
            map.insert(p.param_name.clone(), sn.clone());
        }
    }
    // Top-level locals (override params).
    for stmt in &func.body {
        if let IRStmt::Local { name, short_name: Some(sn), .. } = stmt {
            map.insert(name.clone(), sn.clone());
        }
    }
    map
}

pub(super) fn render_jass_function(func: &IRFunc, global_map: &HashMap<String, String>) -> String {
    let scope_map = build_func_scope_map(func, global_map);
    let fname = decl_name(&func.name, &func.short_name);
    let params = if func.params.is_empty() {
        "nothing".to_string()
    } else {
        func.params.iter().map(|p| {
            format!("{} {}", p.type_name, decl_name(&p.param_name, &p.short_name))
        }).collect::<Vec<_>>().join(", ")
    };
    let mut out = format!("function {} takes {} returns {}\n", fname, params, func.return_type);
    for stmt in &func.body {
        for line in render_jass_stmt(stmt, "    ", &scope_map) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out.push_str("endfunction");
    out
}

// ─── IR local hoisting ───────────────────────────────────────────────────────

/// Default IR value for a JASS type.
///
/// `integer` → `0`, `real` → `0`, `boolean` → `false`,
/// `string` → `""`, everything else → `null`.
fn default_ir_value(type_name: &str) -> IRExpr {
    match type_name {
        "integer" => IRExpr::int(0),
        "real" => IRExpr::int(0),
        "boolean" => IRExpr::bool_val(false),
        "string" => IRExpr::string(""),
        _ => IRExpr::null(),
    }
}

/// Recursively collect `Local` declarations from a statement list.
///
/// Sets `has_late` to `true` if any `Local` is encountered (even for
/// already-declared names that won't be hoisted — they still need to be
/// replaced with `Set` assignments).
fn collect_locals_recursive(
    stmts: &[IRStmt],
    declared: &mut HashSet<String>,
    hoisted: &mut Vec<(String, String, bool)>, // (type_name, name, is_array)
    has_late: &mut bool,
) {
    for stmt in stmts {
        match stmt {
            IRStmt::Local { type_name, is_array, name, .. } => {
                *has_late = true;
                if declared.insert(name.clone()) {
                    hoisted.push((type_name.clone(), name.clone(), *is_array));
                }
            }
            IRStmt::If { body, branches, .. } => {
                collect_locals_recursive(body, declared, hoisted, has_late);
                for b in branches {
                    collect_locals_recursive(&b.body, declared, hoisted, has_late);
                }
            }
            IRStmt::Loop(inner) => {
                collect_locals_recursive(inner, declared, hoisted, has_late);
            }
            _ => {}
        }
    }
}

/// Recursively replace `Local` declarations with `Set` assignments
/// (or remove them entirely when there is no initializer).
fn replace_locals_with_sets(stmts: Vec<IRStmt>) -> Vec<IRStmt> {
    let mut result = Vec::with_capacity(stmts.len());
    for stmt in stmts {
        match stmt {
            IRStmt::Local { name, value: Some(val), .. } => {
                result.push(IRStmt::Set { var: name, index: None, value: val });
            }
            IRStmt::Local { value: None, .. } => {
                // No initializer — drop (hoisted copy is sufficient).
            }
            IRStmt::If { condition, body, branches } => {
                result.push(IRStmt::If {
                    condition,
                    body: replace_locals_with_sets(body),
                    branches: branches.into_iter().map(|b| IRBranch {
                        condition: b.condition,
                        body: replace_locals_with_sets(b.body),
                    }).collect(),
                });
            }
            IRStmt::Loop(inner) => {
                result.push(IRStmt::Loop(replace_locals_with_sets(inner)));
            }
            other => result.push(other),
        }
    }
    result
}

/// Hoist late local declarations in an IR function body to the top.
///
/// In JASS, `local` declarations must appear before any executable statement.
/// Any `Local` found after the first non-`Local` statement (including inside
/// nested `if`/`loop` blocks) is moved to the top of the body with the
/// type's default value, and the original site becomes a `Set` assignment
/// (or is removed if there was no initializer).
pub(super) fn hoist_ir_locals(body: &mut Vec<IRStmt>) {
    // 1. Find split point: first non-Local statement at the top level.
    let split = body.iter()
        .position(|s| !matches!(s, IRStmt::Local { .. }))
        .unwrap_or(body.len());

    // 2. Record names already declared in early locals.
    let mut declared: HashSet<String> = HashSet::new();
    for stmt in &body[..split] {
        if let IRStmt::Local { name, .. } = stmt {
            declared.insert(name.clone());
        }
    }

    // 3. Collect late locals from the rest of the body (recursively).
    let mut hoisted: Vec<(String, String, bool)> = Vec::new();
    let mut has_late = false;
    collect_locals_recursive(&body[split..], &mut declared, &mut hoisted, &mut has_late);

    if !has_late {
        return;
    }

    // 4. Split the body: keep early locals, transform the tail.
    let tail = body.split_off(split);
    let transformed = replace_locals_with_sets(tail);

    // 5. Insert hoisted locals (after existing early locals).
    for (type_name, name, is_array) in hoisted {
        let value = if is_array { None } else { Some(default_ir_value(&type_name)) };
        body.push(IRStmt::Local { type_name, is_array, name, short_name: None, value });
    }

    // 6. Append transformed tail.
    body.extend(transformed);
}

