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
use std::collections::{HashMap, HashSet};

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
pub(super) fn as_rename(name: &str, rename_map: &HashMap<String, String>) -> String {
    rename_map
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.to_string())
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

/// Default literal for an AngelScript type (used for hoisted declarations).
///
/// - `int` → `0`, `float` → `0`, `bool` → `false`, `string` → `""`,
///   everything else → `null`.
pub(super) fn default_for_as_type(as_type: &str) -> &str {
    match as_type {
        "int" => "0",
        "float" => "0",
        "bool" => "false",
        "string" => "\"\"",
        _ => "null",
    }
}

// ─── IR → AngelScript rendering ──────────────────────────────────────────────

/// Render an expression, wrapping any `arr[idx]` table reads with a type cast.
///
/// `table` is untyped, so reading from it requires an explicit cast:
/// `int(myTable[i])`.  The cast is applied to every `Index` node found
/// in the expression tree, while non-index sub-expressions are left as-is.
#[allow(dead_code)]
pub(super) fn render_as_expr_cast(expr: &IRExpr, cast_type: &str) -> String {
    match expr {
        IRExpr::Index { array, index } => {
            format!("{}({}[{}])", cast_type, render_as_expr(array), render_as_expr(index))
        }
        IRExpr::Binary { left, op, right } => {
            let left_str = if op == "and" && matches!(left.as_ref(), IRExpr::Binary { op: o, .. } if o == "or") {
                format!("({})", render_as_expr_cast(left, cast_type))
            } else {
                render_as_expr_cast(left, cast_type)
            };
            let right_str = if op == "and" && matches!(right.as_ref(), IRExpr::Binary { op: o, .. } if o == "or") {
                format!("({})", render_as_expr_cast(right, cast_type))
            } else {
                render_as_expr_cast(right, cast_type)
            };
            format!("{} {} {}", left_str, op, right_str)
        }
        IRExpr::Unary { op, operand } => {
            format!("{} {}", op, render_as_expr_cast(operand, cast_type))
        }
        IRExpr::Parens(inner) => {
            format!("({})", render_as_expr_cast(inner, cast_type))
        }
        _ => render_as_expr(expr),
    }
}

#[allow(dead_code)]
pub(super) fn render_as_expr(expr: &IRExpr) -> String {
    match expr {
        IRExpr::Literal(s) => s.clone(),
        IRExpr::Id(s) => s.clone(),
        IRExpr::Call { name, args } => {
            let a: Vec<String> = args.iter().map(render_as_expr).collect();
            format!("{}({})", name, a.join(", "))
        }
        IRExpr::FuncRef(s) => format!("function {}", s),
        IRExpr::Binary { left, op, right } => {
            // Precedence fix: in JASS `or` binds tighter than `and`,
            // in AS `&&` binds tighter than `||`.  Wrap `or` children of `and`.
            let left_str = if op == "and" && matches!(left.as_ref(), IRExpr::Binary { op: o, .. } if o == "or") {
                format!("({})", render_as_expr(left))
            } else {
                render_as_expr(left)
            };
            let right_str = if op == "and" && matches!(right.as_ref(), IRExpr::Binary { op: o, .. } if o == "or") {
                format!("({})", render_as_expr(right))
            } else {
                render_as_expr(right)
            };
            format!("{} {} {}", left_str, op, right_str)
        }
        IRExpr::Unary { op, operand } => format!("{} {}", op, render_as_expr(operand)),
        IRExpr::Parens(inner) => format!("({})", render_as_expr(inner)),
        IRExpr::Index { array, index } => {
            format!("{}[{}]", render_as_expr(array), render_as_expr(index))
        }
    }
}

#[allow(dead_code)]
pub(super) fn render_as_stmt(stmt: &IRStmt, indent: &str, rename_map: &HashMap<String, String>) -> Vec<String> {
    match stmt {
        IRStmt::Local { type_name, is_array, name, value } => {
            let as_type = jass_type_to_as_type(type_name);
            let as_name = as_rename(name, rename_map);
            if *is_array {
                vec![format!("{}table {} = {{}};", indent, as_name)]
            } else {
                match value {
                    Some(v) => vec![format!("{}{} {} = {};", indent, as_type, as_name, render_as_expr_cast(v, as_type))],
                    None => vec![format!("{}{} {};", indent, as_type, as_name)],
                }
            }
        }
        IRStmt::Set { var, index, value } => {
            let as_var = as_rename(var, rename_map);
            let idx = index.as_ref().map(|i| format!("[{}]", render_as_expr(i))).unwrap_or_default();
            vec![format!("{}{}{} = {};", indent, as_var, idx, render_as_expr(value))]
        }
        IRStmt::Call { name, args } => {
            let as_name = as_rename(name, rename_map);
            let a: Vec<String> = args.iter().map(render_as_expr).collect();
            vec![format!("{}{}({});", indent, as_name, a.join(", "))]
        }
        IRStmt::Return(value) => {
            match value {
                Some(v) => vec![format!("{}return {};", indent, render_as_expr(v))],
                None => vec![format!("{}return;", indent)],
            }
        }
        IRStmt::Exitwhen(cond) => {
            vec![format!("{}if ({}) break;", indent, render_as_expr(cond))]
        }
        IRStmt::If { condition, body, branches } => {
            let inner = format!("{}    ", indent);
            let mut lines = vec![format!("{}if ({}) {{", indent, render_as_expr(condition))];
            for s in body { lines.extend(render_as_stmt(s, &inner, rename_map)); }
            for b in branches {
                if let Some(ref cond) = b.condition {
                    lines.push(format!("{}}} else if ({}) {{", indent, render_as_expr(cond)));
                } else {
                    lines.push(format!("{}}} else {{", indent));
                }
                for s in &b.body { lines.extend(render_as_stmt(s, &inner, rename_map)); }
            }
            lines.push(format!("{}}}", indent));
            lines
        }
        IRStmt::Loop(body) => {
            let inner = format!("{}    ", indent);
            let mut lines = vec![format!("{}while (true) {{", indent)];
            for s in body { lines.extend(render_as_stmt(s, &inner, rename_map)); }
            lines.push(format!("{}}}", indent));
            lines
        }
        IRStmt::VarDecl { is_constant: _, is_array, type_name, decls } => {
            let as_type = jass_type_to_as_type(type_name);
            decls.iter().map(|d| {
                let as_name = as_rename(&d.name, rename_map);
                if *is_array {
                    format!("{}table {} = {{}};", indent, as_name)
                } else {
                    match &d.value {
                        Some(v) => format!("{}{} {} = {};", indent, as_type, as_name, render_as_expr(v)),
                        None => format!("{}{} {};", indent, as_type, as_name),
                    }
                }
            }).collect()
        }
    }
}

#[allow(dead_code)]
pub(super) fn render_as_function(func: &IRFunc, rename_map: &HashMap<String, String>) -> String {
    let as_ret = jass_type_to_as_type(&func.return_type);
    let as_name = as_rename(&func.name, rename_map);
    let as_params = if func.params.is_empty() {
        String::new()
    } else {
        func.params.iter().map(|(t, n)| {
            format!("{} {}", jass_type_to_as_type(t), as_rename(n, rename_map))
        }).collect::<Vec<_>>().join(", ")
    };
    let mut out = format!("{} {}({}) {{\n", as_ret, as_name, as_params);
    for stmt in &func.body {
        for line in render_as_stmt(stmt, "    ", rename_map) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out.push('}');
    out
}

