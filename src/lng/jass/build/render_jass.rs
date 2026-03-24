//! IR → JASS rendering.
//!
//! Converts IR nodes back into JASS source text.  Also contains the
//! local-hoisting pass that ensures `local` declarations appear before
//! any executable statement (a JASS requirement).

use super::ir::*;
use super::render_as::jass_type_to_as_type;
use std::collections::HashSet;

// ─── IR → JASS rendering ────────────────────────────────────────────────────

pub(super) fn render_jass_expr(expr: &IRExpr) -> String {
    match expr {
        IRExpr::Literal(s) => s.clone(),
        IRExpr::Id(s) => s.clone(),
        IRExpr::Call { name, args } => {
            let a: Vec<String> = args.iter().map(render_jass_expr).collect();
            format!("{}({})", name, a.join(", "))
        }
        IRExpr::FuncRef(s) => format!("function {}", s),
        IRExpr::Binary { left, op, right } => {
            // Parenthesise `or` operands of `and` so that JASS precedence
            // (where `or` binds tighter) is preserved when the text is later
            // converted to AngelScript (where `and` binds tighter).
            // In JASS the parens are redundant but harmless.
            let left_str = if op == "and" && matches!(left.as_ref(), IRExpr::Binary { op: o, .. } if o == "or") {
                format!("({})", render_jass_expr(left))
            } else {
                render_jass_expr(left)
            };
            let right_str = if op == "and" && matches!(right.as_ref(), IRExpr::Binary { op: o, .. } if o == "or") {
                format!("({})", render_jass_expr(right))
            } else {
                render_jass_expr(right)
            };
            format!("{} {} {}", left_str, op, right_str)
        }
        IRExpr::Unary { op, operand } => {
            format!("{} {}", op, render_jass_expr(operand))
        }
        IRExpr::Parens(inner) => format!("({})", render_jass_expr(inner)),
        IRExpr::Index { array, index } => {
            format!("{}[{}]", render_jass_expr(array), render_jass_expr(index))
        }
        IRExpr::Cast { type_name, inner } => {
            format!("{}({})", jass_type_to_as_type(type_name), render_jass_expr(inner))
        }
    }
}

pub(super) fn render_jass_stmt(stmt: &IRStmt, indent: &str) -> Vec<String> {
    match stmt {
        IRStmt::Local { type_name, is_array, name, value } => {
            let arr = if *is_array { " array" } else { "" };
            match value {
                Some(v) => vec![format!("{}local {}{} {} = {}", indent, type_name, arr, name, render_jass_expr(v))],
                None => vec![format!("{}local {}{} {}", indent, type_name, arr, name)],
            }
        }
        IRStmt::Set { var, index, value } => {
            let idx = index.as_ref().map(|i| format!("[{}]", render_jass_expr(i))).unwrap_or_default();
            vec![format!("{}set {}{} = {}", indent, var, idx, render_jass_expr(value))]
        }
        IRStmt::Call { name, args } => {
            let a: Vec<String> = args.iter().map(render_jass_expr).collect();
            vec![format!("{}call {}({})", indent, name, a.join(", "))]
        }
        IRStmt::Return(value) => {
            match value {
                Some(v) => vec![format!("{}return {}", indent, render_jass_expr(v))],
                None => vec![format!("{}return", indent)],
            }
        }
        IRStmt::Exitwhen(cond) => {
            vec![format!("{}exitwhen {}", indent, render_jass_expr(cond))]
        }
        IRStmt::If { condition, body, branches } => {
            let inner = format!("{}    ", indent);
            let mut lines = vec![format!("{}if {} then", indent, render_jass_expr(condition))];
            for s in body { lines.extend(render_jass_stmt(s, &inner)); }
            for b in branches {
                if let Some(ref cond) = b.condition {
                    lines.push(format!("{}elseif {} then", indent, render_jass_expr(cond)));
                } else {
                    lines.push(format!("{}else", indent));
                }
                for s in &b.body { lines.extend(render_jass_stmt(s, &inner)); }
            }
            lines.push(format!("{}endif", indent));
            lines
        }
        IRStmt::Loop(body) => {
            let inner = format!("{}    ", indent);
            let mut lines = vec![format!("{}loop", indent)];
            for s in body { lines.extend(render_jass_stmt(s, &inner)); }
            lines.push(format!("{}endloop", indent));
            lines
        }
        IRStmt::VarDecl { is_constant, is_array, type_name, decls } => {
            let mut prefix = String::new();
            if *is_constant { prefix.push_str("constant "); }
            prefix.push_str(type_name);
            if *is_array { prefix.push_str(" array"); }
            let d: Vec<String> = decls.iter().map(|d| {
                match &d.value {
                    Some(v) => format!("{} = {}", d.name, render_jass_expr(v)),
                    None => d.name.clone(),
                }
            }).collect();
            vec![format!("{}{} {}", indent, prefix, d.join(", "))]
        }
    }
}

pub(super) fn render_jass_function(func: &IRFunc) -> String {
    let params = if func.params.is_empty() {
        "nothing".to_string()
    } else {
        func.params.iter().map(|(t, n)| format!("{} {}", t, n)).collect::<Vec<_>>().join(", ")
    };
    let mut out = format!("function {} takes {} returns {}\n", func.name, params, func.return_type);
    for stmt in &func.body {
        for line in render_jass_stmt(stmt, "    ") {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out.push_str("endfunction");
    out
}

// ─── JASS local hoisting ─────────────────────────────────────────────────────

/// Default literal for a JASS type (used for hoisted local declarations).
///
/// - `integer` → `0`, `real` → `0`, `boolean` → `false`,
///   `string` → `""`, everything else → `null`.
fn default_for_jass_type(jass_type: &str) -> &str {
    match jass_type {
        "integer" => "0",
        "real" => "0",
        "boolean" => "false",
        "string" => "\"\"",
        _ => "null",
    }
}

/// Extract type / name pairs from a declaration line (JASS-side hoisting).
///
/// Returns `Vec<(jass_type, name, is_array)>`.
fn extract_jass_hoisted_vars(trimmed: &str) -> Vec<(String, String, bool)> {
    let mut t = trimmed;
    t = t.strip_prefix("local ").unwrap_or(t);
    t = t.strip_prefix("constant ").unwrap_or(t);

    let mut parts = t.splitn(2, ' ');
    let type_name = parts.next().unwrap_or("integer").to_string();
    let rest = parts.next().unwrap_or("");

    let (is_array, rest) = if let Some(r) = rest.strip_prefix("array ") {
        (true, r)
    } else {
        (false, rest)
    };

    rest.split(',')
        .filter_map(|decl| {
            let name = decl.trim().split('=').next()?.trim()
                .split_whitespace().next()?;
            if name.is_empty() { return None; }
            Some((type_name.clone(), name.to_string(), is_array))
        })
        .collect()
}

/// Convert a hoisted JASS variable declaration into `set NAME = VALUE` lines.
///
/// If there is no initialiser the line is omitted (the hoisted `local`
/// at the top is sufficient).
fn jass_var_decl_to_set_assignments(line: &str) -> Vec<String> {
    let indent = &line[..line.len() - line.trim_start().len()];
    let mut t = line.trim();
    t = t.strip_prefix("local ").unwrap_or(t).trim();
    t = t.strip_prefix("constant ").unwrap_or(t).trim();

    // Skip type name.
    let mut parts = t.splitn(2, ' ');
    let _type = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("");
    let rest = rest.strip_prefix("array ").unwrap_or(rest);

    rest.split(',')
        .filter_map(|decl| {
            let decl = decl.trim();
            let eq_pos = decl.find('=')?;
            let name = decl[..eq_pos].trim();
            let value = decl[eq_pos + 1..].trim();
            Some(format!("{}set {} = {}", indent, name, value))
        })
        .collect()
}

/// Determine whether a trimmed body line is a variable declaration.
///
/// Returns `true` for lines like `local TYPE NAME`, `TYPE NAME = …`,
/// `constant TYPE array NAME`, etc.  Returns `false` for known
/// statement keywords (`set`, `call`, `return`, `exitwhen`, `if`,
/// `loop`, etc.) and control-flow markers.
pub(super) fn is_var_decl_line(trimmed: &str) -> bool {
    if trimmed.starts_with("local ") || trimmed.starts_with("constant ") {
        return true;
    }
    if trimmed.is_empty()
        || trimmed.starts_with("set ")
        || trimmed.starts_with("call ")
        || trimmed.starts_with("return")
        || trimmed.starts_with("exitwhen ")
        || trimmed == "loop"
        || trimmed == "endloop"
        || trimmed.starts_with("if ")
        || trimmed.starts_with("elseif ")
        || trimmed == "else"
        || trimmed == "endif"
        || trimmed == "endfunction"
    {
        return false;
    }
    // Must have at least TYPE + NAME (two whitespace-separated tokens).
    trimmed.split_whitespace().count() >= 2
}

/// Hoist late local declarations in a JASS function source to the top.
///
/// In JASS, `local` declarations must appear before any other statement.
/// Any variable declaration found after the first non-declaration
/// statement is moved to the top of the function body (with the type's
/// default value), and the original site becomes a plain `set` assignment.
pub(super) fn hoist_jass_locals(source: &str) -> String {
    let mut lines_iter = source.lines();
    let sig = match lines_iter.next() {
        Some(l) => l,
        None => return source.to_string(),
    };

    let body_lines: Vec<&str> = lines_iter.collect();

    // ── Pass 1: find declarations that appear after the first instruction ──
    // Track all declared variable names to avoid duplicate hoisted locals.
    let mut declared_names: HashSet<String> = HashSet::new();
    let mut seen_instruction = false;
    let mut hoisted: Vec<(String, String, bool)> = Vec::new();
    let mut has_late_decls = false;

    for line in &body_lines {
        let t = line.trim();
        if t.is_empty() || t == "endfunction" {
            continue;
        }
        if is_var_decl_line(t) {
            let vars = extract_jass_hoisted_vars(t);
            if seen_instruction {
                has_late_decls = true;
                for v in vars {
                    if declared_names.insert(v.1.clone()) {
                        hoisted.push(v);
                    }
                }
            } else {
                // Early declarations — just record the names.
                for v in &vars {
                    declared_names.insert(v.1.clone());
                }
            }
        } else {
            seen_instruction = true;
        }
    }

    if !has_late_decls {
        return source.to_string();
    }

    let mut out = String::from(sig);

    // Emit hoisted declarations right after the signature.
    for (type_name, var_name, is_array) in &hoisted {
        out.push('\n');
        if *is_array {
            out.push_str(&format!("    local {} array {}", type_name, var_name));
        } else {
            out.push_str(&format!(
                "    local {} {} = {}",
                type_name,
                var_name,
                default_for_jass_type(type_name),
            ));
        }
    }

    // ── Pass 2: emit body, converting hoisted decls to `set` assignments ──
    seen_instruction = false;
    for line in &body_lines {
        let t = line.trim();
        if t == "endfunction" {
            out.push('\n');
            out.push_str("endfunction");
            continue;
        }
        if t.is_empty() {
            out.push('\n');
            continue;
        }

        if is_var_decl_line(t) && seen_instruction {
            // Hoisted — emit only the set assignment(s), if any.
            for a in jass_var_decl_to_set_assignments(line) {
                out.push('\n');
                out.push_str(&a);
            }
        } else {
            if !is_var_decl_line(t) {
                seen_instruction = true;
            }
            out.push('\n');
            out.push_str(line);
        }
    }

    out
}

/// Test-only wrapper for [`hoist_jass_locals`].
#[cfg(test)]
pub fn hoist_jass_locals_text(source: &str) -> String {
    hoist_jass_locals(source)
}

