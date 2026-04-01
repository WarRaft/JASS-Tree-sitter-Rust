//! IR → AngelScript rendering.
//!
//! Converts IR nodes into AngelScript source text.  Also contains
//! type-mapping utilities (`jass_type_to_as_type`), reserved-word
//! renaming (`AS_RESERVED`, `build_as_rename_map`, `as_rename`), and
//! default-value helpers used by both the IR renderer and the text-based
//! JASS→AS converter.
//!
//! **Future direction**: when the binary→AS path is active, `augment_config`
//! and `augment_main` will populate IR nodes that are rendered here directly
//! instead of going through JASS text first.

use super::ir::*;
use super::render_jass::build_func_scope_map;
use std::collections::{HashMap, HashSet};

// Re-import target lang bitmask.
use super::ir::TARGET_AS;

// ─── Type / rename utilities ─────────────────────────────────────────────────

/// AngelScript reserved words that cannot be used as identifiers.
pub(super) const AS_RESERVED: &[&str] = &[
    "and", "abstract", "auto", "bool", "break", "case", "cast", "catch", "class",
    "const", "continue", "default", "do", "double", "else", "enum", "explicit",
    "external", "false", "final", "float", "for", "from", "funcdef", "get",
    "if", "import", "in", "inout", "int", "interface", "int8", "int16", "int32",
    "int64", "is", "mixin", "namespace", "not", "null", "or", "out", "override",
    "private", "property", "protected", "return", "set", "shared", "super",
    "switch", "this", "true", "try", "typedef", "uint", "uint8", "uint16",
    "uint32", "uint64", "void", "while", "xor",
];

/// Build a rename map: for each name that is an AS reserved word,
/// generate `name1`, `name2`, … until no collision.
#[allow(dead_code)]
pub(super) fn build_as_rename_map(names: &[&str]) -> HashMap<String, String> {
    let reserved: HashSet<&str> = AS_RESERVED.iter().copied().collect();
    let all: HashSet<&str> = names.iter().copied().collect();
    let mut map = HashMap::new();

    for &name in names {
        if reserved.contains(name) {
            let mut suffix = 1u32;
            loop {
                let candidate = format!("{}{}", name, suffix);
                if !reserved.contains(candidate.as_str()) && !all.contains(candidate.as_str()) {
                    map.insert(name.to_string(), candidate);
                    break;
                }
                suffix += 1;
            }
        }
    }
    map
}

/// Rename an identifier if it collides with AS reserved words.
#[allow(dead_code)]
pub(super) fn as_rename(name: &str, rename_map: &HashMap<String, String>) -> String {
    ir_rename(name, rename_map)
}

/// Map a JASS type name to an AS type name.
pub(super) fn jass_type_to_as_type(t: &str) -> &str {
    match t {
        "integer" => "int",
        "real" => "float",
        "boolean" => "bool",
        "string" => "string",
        "nothing" => "void",
        "code" => "funcdef",
        other => other,
    }
}


// ─── IR → AngelScript rendering ──────────────────────────────────────────────

/// Render an expression, wrapping any `arr[idx]` table reads with a type cast.
///
/// `table` is untyped, so reading from it requires an explicit cast:
/// `int(myTable[i])`.  The cast is applied to every `Index` node found
/// in the expression tree, while non-index sub-expressions are left as-is.
pub(super) fn render_as_expr_cast(expr: &IRExpr, cast_type: &str, rename_map: &HashMap<String, String>) -> String {
    match expr {
        IRExpr::Index { array, index } => {
            format!("{}({}[{}])", cast_type, render_as_expr(array, rename_map), render_as_expr(index, rename_map))
        }
        IRExpr::Binary { left, op, right } => {
            let left_str = if op == "and" && matches!(left.as_ref(), IRExpr::Binary { op: o, .. } if o == "or") {
                format!("({})", render_as_expr_cast(left, cast_type, rename_map))
            } else {
                render_as_expr_cast(left, cast_type, rename_map)
            };
            let right_str = if op == "and" && matches!(right.as_ref(), IRExpr::Binary { op: o, .. } if o == "or") {
                format!("({})", render_as_expr_cast(right, cast_type, rename_map))
            } else {
                render_as_expr_cast(right, cast_type, rename_map)
            };
            format!("{} {} {}", left_str, op, right_str)
        }
        IRExpr::Unary { op, operand } => {
            format!("{} {}", op, render_as_expr_cast(operand, cast_type, rename_map))
        }
        IRExpr::Parens(inner) => {
            format!("({})", render_as_expr_cast(inner, cast_type, rename_map))
        }
        _ => render_as_expr(expr, rename_map),
    }
}

pub(super) fn render_as_expr(expr: &IRExpr, rn: &HashMap<String, String>) -> String {
    match expr {
        IRExpr::Literal(s) => s.clone(),
        IRExpr::Id(s) => ir_rename(s, rn),
        IRExpr::Call { name, args } => {
            let n = ir_rename(name, rn);
            let a: Vec<String> = args.iter().map(|a| render_as_expr(a, rn)).collect();
            format!("{}({})", n, a.join(", "))
        }
        IRExpr::FuncRef(s) => format!("@{}", ir_rename(s, rn)),
        IRExpr::Binary { left, op, right } => {
            // Precedence fix: in JASS `or` binds tighter than `and`,
            // in AS `&&` binds tighter than `||`.  Wrap `or` children of `and`.
            let left_str = if op == "and" && matches!(left.as_ref(), IRExpr::Binary { op: o, .. } if o == "or") {
                format!("({})", render_as_expr(left, rn))
            } else {
                render_as_expr(left, rn)
            };
            let right_str = if op == "and" && matches!(right.as_ref(), IRExpr::Binary { op: o, .. } if o == "or") {
                format!("({})", render_as_expr(right, rn))
            } else {
                render_as_expr(right, rn)
            };
            format!("{} {} {}", left_str, op, right_str)
        }
        IRExpr::Unary { op, operand } => format!("{} {}", op, render_as_expr(operand, rn)),
        IRExpr::Parens(inner) => format!("({})", render_as_expr(inner, rn)),
        IRExpr::Index { array, index } => {
            format!("{}[{}]", render_as_expr(array, rn), render_as_expr(index, rn))
        }
        IRExpr::Cast { type_name, inner } => {
            format!("{}({})", jass_type_to_as_type(type_name), render_as_expr(inner, rn))
        }
    }
}

pub(super) fn render_as_stmt(stmt: &IRStmt, indent: &str, rn: &HashMap<String, String>) -> Vec<String> {
    match stmt {
        IRStmt::Local { type_name, is_array, name, short_name, value } => {
            let as_type = jass_type_to_as_type(type_name);
            let dn = decl_name(name, short_name);
            if *is_array {
                vec![format!("{}table {} = {{}};", indent, dn)]
            } else {
                match value {
                    Some(v) => vec![format!("{}{} {} = {};", indent, as_type, dn, render_as_expr_cast(v, as_type, rn))],
                    None => vec![format!("{}{} {};", indent, as_type, dn)],
                }
            }
        }
        IRStmt::Set { var, index, value } => {
            let v = ir_rename(var, rn);
            let idx = index.as_ref().map(|i| format!("[{}]", render_as_expr(i, rn))).unwrap_or_default();
            vec![format!("{}{}{} = {};", indent, v, idx, render_as_expr(value, rn))]
        }
        IRStmt::Call { name, args } => {
            let n = ir_rename(name, rn);
            let a: Vec<String> = args.iter().map(|a| render_as_expr(a, rn)).collect();
            vec![format!("{}{}({});", indent, n, a.join(", "))]
        }
        IRStmt::Return(value) => {
            match value {
                Some(v) => vec![format!("{}return {};", indent, render_as_expr(v, rn))],
                None => vec![format!("{}return;", indent)],
            }
        }
        IRStmt::Exitwhen(cond) => {
            vec![format!("{}if ({}) break;", indent, render_as_expr(cond, rn))]
        }
        IRStmt::If { condition, body, branches } => {
            let inner = format!("{}    ", indent);
            let mut lines = vec![format!("{}if ({}) {{", indent, render_as_expr(condition, rn))];
            for s in body { lines.extend(render_as_stmt(s, &inner, rn)); }
            for b in branches {
                if let Some(ref cond) = b.condition {
                    lines.push(format!("{}}} else if ({}) {{", indent, render_as_expr(cond, rn)));
                } else {
                    lines.push(format!("{}}} else {{", indent));
                }
                for s in &b.body { lines.extend(render_as_stmt(s, &inner, rn)); }
            }
            lines.push(format!("{}}}", indent));
            lines
        }
        IRStmt::Loop(body) => {
            let inner = format!("{}    ", indent);
            let mut lines = vec![format!("{}while (true) {{", indent)];
            for s in body { lines.extend(render_as_stmt(s, &inner, rn)); }
            lines.push(format!("{}}}", indent));
            lines
        }
        IRStmt::VarDecl { is_constant: _, is_array, type_name, decls } => {
            let as_type = jass_type_to_as_type(type_name);
            decls.iter().map(|d| {
                let dn = decl_name(&d.name, &d.short_name);
                if *is_array {
                    format!("{}table {} = {{}};", indent, dn)
                } else {
                    match &d.value {
                        Some(v) => format!("{}{} {} = {};", indent, as_type, dn, render_as_expr(v, rn)),
                        None => format!("{}{} {};", indent, as_type, dn),
                    }
                }
            }).collect()
        }
        IRStmt::TargetOnly { target, inner } => {
            if *target & TARGET_AS != 0 {
                render_as_stmt(inner, indent, rn)
            } else {
                vec![]
            }
        }
    }
}

pub(super) fn render_as_function(func: &IRFunc, global_map: &HashMap<String, String>) -> String {
    let scope_map = build_func_scope_map(func, global_map);
    let as_ret = jass_type_to_as_type(&func.return_type);
    let fname = decl_name(&func.name, &func.short_name);
    let as_params = if func.params.is_empty() {
        String::new()
    } else {
        func.params.iter().map(|p| {
            format!("{} {}", jass_type_to_as_type(&p.type_name), decl_name(&p.param_name, &p.short_name))
        }).collect::<Vec<_>>().join(", ")
    };
    let mut out = format!("{} {}({}) {{\n", as_ret, fname, as_params);
    for stmt in &func.body {
        for line in render_as_stmt(stmt, "    ", &scope_map) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out.push('}');
    out
}

