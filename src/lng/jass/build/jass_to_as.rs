//! Text-based JASS → AngelScript conversion.
//!
//! Operates on rendered JASS source text (not the IR) and transforms it
//! line-by-line into AngelScript syntax.  Handles local hoisting (for late
//! declarations), identifier renaming (AS reserved words), and all
//! statement-level syntax differences.

use super::render_as::{as_rename, default_for_as_type, jass_type_to_as_type};
use super::render_jass::is_var_decl_line;
use std::collections::{HashMap, HashSet};

/// Apply renames to identifiers in a line of code and convert JASS
/// function references (`function NAME`) to AS syntax (`@NAME`).
pub(super) fn apply_rename_to_line(line: &str, rename_map: &HashMap<String, String>) -> String {
    let mut result = String::with_capacity(line.len());
    let mut chars = line.char_indices().peekable();

    while let Some((start, ch)) = chars.next() {
        if ch.is_ascii_alphabetic() || ch == '_' {
            // Collect the full identifier
            let mut end = start + ch.len_utf8();
            while let Some(&(_, next_ch)) = chars.peek() {
                if next_ch.is_ascii_alphanumeric() || next_ch == '_' {
                    end += next_ch.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let word = &line[start..end];

            // Convert JASS `function NAME` → AS `@NAME`.
            if word == "function" {
                if let Some(&(_, ' ')) = chars.peek() {
                    chars.next(); // consume space
                    if let Some(&(id_start, id_ch)) = chars.peek() {
                        if id_ch.is_ascii_alphabetic() || id_ch == '_' {
                            chars.next();
                            let mut id_end = id_start + id_ch.len_utf8();
                            while let Some(&(_, nc)) = chars.peek() {
                                if nc.is_ascii_alphanumeric() || nc == '_' {
                                    id_end += nc.len_utf8();
                                    chars.next();
                                } else {
                                    break;
                                }
                            }
                            let func_name = &line[id_start..id_end];
                            result.push('@');
                            if let Some(replacement) = rename_map.get(func_name) {
                                result.push_str(replacement);
                            } else {
                                result.push_str(func_name);
                            }
                            continue;
                        }
                    }
                    // `function` followed by space but not an identifier — keep as-is.
                    result.push_str("function ");
                    continue;
                }
            }

            if let Some(replacement) = rename_map.get(word) {
                result.push_str(replacement);
            } else {
                result.push_str(word);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Extract type / name pairs from a declaration line for hoisting.
///
/// Handles: `[local] [constant] TYPE [array] NAME [= VAL][, NAME2 [= VAL2]]`
///
/// Returns `Vec<(as_type, as_name, is_array)>`.
fn extract_hoisted_vars(
    trimmed: &str,
    rename_map: &HashMap<String, String>,
) -> Vec<(String, String, bool)> {
    let mut t = trimmed;
    t = t.strip_prefix("local ").unwrap_or(t);
    t = t.strip_prefix("constant ").unwrap_or(t);

    let mut parts = t.splitn(2, ' ');
    let type_name = parts.next().unwrap_or("integer");
    let rest = parts.next().unwrap_or("");

    let (is_array, rest) = if let Some(r) = rest.strip_prefix("array ") {
        (true, r)
    } else {
        (false, rest)
    };

    let as_type = jass_type_to_as_type(type_name).to_string();

    rest.split(',')
        .filter_map(|decl| {
            let name = decl.trim().split('=').next()?.trim()
                .split_whitespace().next()?;
            if name.is_empty() { return None; }
            Some((as_type.clone(), as_rename(name, rename_map), is_array))
        })
        .collect()
}

/// Convert a hoisted variable declaration line into plain assignment(s).
///
/// If the declaration has an initialiser (`TYPE NAME = VALUE`), returns
/// `NAME = VALUE;`.  If there is no initialiser, returns nothing (the
/// hoisted declaration at the top is sufficient).
fn var_decl_to_assignments(
    line: &str,
    rename_map: &HashMap<String, String>,
) -> Vec<String> {
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
            let name = decl[..eq_pos].trim().split_whitespace().next()?;
            let value = decl[eq_pos + 1..].trim();
            Some(format!(
                "{}{} = {};",
                indent,
                as_rename(name, rename_map),
                apply_rename_to_line(value, rename_map),
            ))
        })
        .collect()
}

/// Convert a JASS function to AS syntax.
///
/// Performs a two-pass conversion:
/// 1. **Scan** all body lines to find variable declarations that appear
///    *after* the first non-declaration statement — those must be hoisted
///    to the top of the function with their type's default value, because
///    in JASS locals are only legal at the very top.
/// 2. **Emit** the converted body, replacing hoisted declarations with
///    plain assignments (or nothing if there was no initialiser).
pub(super) fn jass_function_to_as(source: &str, rename_map: &HashMap<String, String>) -> String {
    let mut lines_iter = source.lines();
    let mut out = String::new();

    // First line: function Foo takes ... returns ...
    if let Some(first) = lines_iter.next() {
        let trimmed = first.trim();
        let rest = trimmed
            .strip_prefix("function ")
            .unwrap_or(trimmed);
        let sig = convert_func_signature(rest, rename_map);
        out.push_str(&sig);
        out.push_str(" {");
    }

    let body_lines: Vec<&str> = lines_iter.collect();

    // ── Pass 1: find declarations that appear after the first instruction ──
    // Track all declared variable names to avoid duplicate hoisted locals.
    let mut declared_names: HashSet<String> = HashSet::new();
    let mut seen_instruction = false;
    let mut hoisted: Vec<(String, String, bool)> = Vec::new(); // (as_type, as_name, is_array)

    for line in &body_lines {
        let t = line.trim();
        if t.is_empty() || t == "endfunction" {
            continue;
        }
        if is_var_decl_line(t) {
            let vars = extract_hoisted_vars(t, rename_map);
            if seen_instruction {
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

    // Emit hoisted declarations with default values right after the opening brace.
    for (as_type, as_name, is_array) in &hoisted {
        out.push('\n');
        if *is_array {
            out.push_str(&format!("    table {} = {{}};", as_name));
        } else {
            out.push_str(&format!(
                "    {} {} = {};",
                as_type,
                as_name,
                default_for_as_type(as_type),
            ));
        }
    }

    // ── Pass 2: emit body lines ──
    seen_instruction = false;
    for line in &body_lines {
        let t = line.trim();
        if t == "endfunction" {
            out.push('\n');
            out.push('}');
            continue;
        }
        if t.is_empty() {
            out.push('\n');
            continue;
        }

        if is_var_decl_line(t) && seen_instruction {
            // Hoisted — emit only the assignment(s), if any.
            for a in var_decl_to_assignments(line, rename_map) {
                out.push('\n');
                out.push_str(&a);
            }
        } else {
            if !is_var_decl_line(t) {
                seen_instruction = true;
            }
            out.push('\n');
            out.push_str(&jass_body_line_to_as(line, rename_map));
        }
    }

    out
}

/// Convert a JASS statement line to AS (inside function body).
fn jass_body_line_to_as(line: &str, rename_map: &HashMap<String, String>) -> String {
    let indent = &line[..line.len() - line.trim_start().len()];
    let t = line.trim();

    if t.is_empty() {
        return String::new();
    }

    // local type name [= value]
    if let Some(rest) = t.strip_prefix("local ") {
        return format!("{}{};", indent, jass_var_decl_to_as_inner(rest, rename_map));
    }

    // set var = value → var = value;
    if let Some(rest) = t.strip_prefix("set ") {
        return format!("{}{};", indent, apply_rename_to_line(rest, rename_map));
    }

    // call Foo(args) → Foo(args);
    if let Some(rest) = t.strip_prefix("call ") {
        return format!("{}{};", indent, apply_rename_to_line(rest, rename_map));
    }

    // return [value]
    if t == "return" {
        return format!("{}return;", indent);
    }
    if let Some(rest) = t.strip_prefix("return ") {
        return format!("{}return {};", indent, apply_rename_to_line(rest, rename_map));
    }

    // exitwhen cond → if (cond) break;
    if let Some(rest) = t.strip_prefix("exitwhen ") {
        return format!("{}if ({}) break;", indent, apply_rename_to_line(rest, rename_map));
    }

    // loop → while (true) {
    if t == "loop" {
        return format!("{}while (true) {{", indent);
    }
    // endloop → }
    if t == "endloop" {
        return format!("{}}}", indent);
    }

    // if cond then → if (cond) {
    if t.starts_with("if ") && t.ends_with(" then") {
        let cond = &t[3..t.len() - 5];
        return format!("{}if ({}) {{", indent, apply_rename_to_line(cond.trim(), rename_map));
    }
    // elseif cond then → } else if (cond) {
    if t.starts_with("elseif ") && t.ends_with(" then") {
        let cond = &t[7..t.len() - 5];
        return format!("{}}} else if ({}) {{", indent, apply_rename_to_line(cond.trim(), rename_map));
    }
    // else → } else {
    if t == "else" {
        return format!("{}}} else {{", indent);
    }
    // endif → }
    if t == "endif" {
        return format!("{}}}", indent);
    }

    // Bare variable declaration (VarStmt without `local` keyword).
    // e.g., "integer x = 5", "constant real pi = 3.14", "unit array arr"
    format!("{}{};", indent, jass_var_decl_to_as_inner(t, rename_map))
}

/// Convert `Foo takes type1 a, type2 b returns type3` → `type3 Foo(type1 a, type2 b)`
fn convert_func_signature(
    rest: &str,
    rename_map: &HashMap<String, String>,
) -> String {
    // Split: name takes params returns ret_type
    let parts: Vec<&str> = rest.splitn(2, " takes ").collect();
    let name = if parts.is_empty() {
        rest
    } else {
        parts[0].trim()
    };
    let as_name = as_rename(name, rename_map);

    let after_takes = if parts.len() > 1 { parts[1] } else { "nothing returns nothing" };

    let (params_str, ret_type) = if let Some(pos) = after_takes.find(" returns ") {
        (&after_takes[..pos], after_takes[pos + 9..].trim())
    } else {
        (after_takes, "nothing")
    };

    let as_ret = jass_type_to_as_type(ret_type);

    let as_params = if params_str.trim() == "nothing" {
        String::new()
    } else {
        params_str
            .split(',')
            .map(|p| {
                let p = p.trim();
                let mut parts = p.splitn(2, ' ');
                let type_name = parts.next().unwrap_or("int");
                let param_name = parts.next().unwrap_or("_");
                format!("{} {}", jass_type_to_as_type(type_name), as_rename(param_name, rename_map))
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    format!("{} {}({})", as_ret, as_name, as_params)
}

/// Convert a JASS var declaration to AS (global scope).
pub(super) fn jass_var_decl_to_as(decl: &str, rename_map: &HashMap<String, String>) -> String {
    format!("{};", jass_var_decl_to_as_inner(decl, rename_map))
}

/// Inner: `integer a = 5` → `int a = 5`
fn jass_var_decl_to_as_inner(decl: &str, rename_map: &HashMap<String, String>) -> String {
    let t = decl.trim();
    // Strip optional leading keywords
    let t = t.strip_prefix("constant ").unwrap_or(t);

    // Split into tokens: type name [= value]
    let mut tokens = t.splitn(2, ' ');
    let type_name = tokens.next().unwrap_or("int");
    let rest = tokens.next().unwrap_or("");

    let (is_array, rest) = if let Some(r) = rest.strip_prefix("array ") {
        (true, r)
    } else {
        (false, rest)
    };

    let as_type = jass_type_to_as_type(type_name);
    if is_array {
        format!("table {} = {{}}", apply_rename_to_line(rest, rename_map))
    } else {
        format!("{} {}", as_type, apply_rename_to_line(rest, rename_map))
    }
}

// ─── Test-only wrappers ──────────────────────────────────────────────────────

/// Test-only: convert a JASS function source to AS via the text pipeline.
#[cfg(test)]
pub fn jass_function_to_as_text(jass_source: &str) -> String {
    let rename_map = HashMap::new();
    jass_function_to_as(jass_source, &rename_map)
}

/// Test-only: convert a JASS global var declaration to AS.
#[cfg(test)]
pub fn jass_var_decl_to_as_text(decl: &str) -> String {
    let rename_map = HashMap::new();
    jass_var_decl_to_as(decl, &rename_map)
}
