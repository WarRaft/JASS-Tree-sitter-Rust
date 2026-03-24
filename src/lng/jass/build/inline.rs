//! Function inlining and StringHash folding.
//!
//! Provides the inline-candidate detection, single-call-site inlining pass,
//! and compile-time `StringHash(…)` folding across all build fragments.

use crate::util::file_store::FILE_STORE;
use crate::util::string_hash::{collect_constants, fold_string_hash, fold_string_integer_args};
use std::collections::HashMap;

use super::ir::*;
use super::render_jass::render_jass_expr;

// ─── Inline candidate detection ──────────────────────────────────────────────

/// Detect whether a function body is a single `return expr` and, if so,
/// return the [`InlineCandidate`] with the expression text and compoundness.
///
/// Operates on the owned IR (not the CST), so it can be called after
/// `convert_body` has already transformed the AST into IR nodes.
pub(super) fn detect_inline_candidate(
    body: &[IRStmt],
) -> Option<InlineCandidate> {
    // Body must contain exactly one statement: `return <expr>`.
    if body.len() != 1 {
        return None;
    }
    if let IRStmt::Return(Some(expr)) = &body[0] {
        let expr_text = render_jass_expr(expr);
        let is_compound = matches!(expr, IRExpr::Binary { .. } | IRExpr::Unary { .. });
        Some(InlineCandidate { expr_text, is_compound })
    } else {
        None
    }
}

// ─── Call-site counting and checking ─────────────────────────────────────────

/// Count occurrences of `NAME()` with word-boundary check in `source`.
fn count_call_occurrences(source: &str, func_name: &str) -> usize {
    let pattern = format!("{}()", func_name);
    let mut count = 0;
    let mut search_from = 0;
    while let Some(pos) = source[search_from..].find(&pattern) {
        let abs_pos = search_from + pos;
        let is_boundary = if abs_pos == 0 {
            true
        } else {
            let b = source.as_bytes()[abs_pos - 1];
            !b.is_ascii_alphanumeric() && b != b'_'
        };
        if is_boundary {
            count += 1;
        }
        search_from = abs_pos + pattern.len();
    }
    count
}

/// Check whether `NAME()` at the given position in `source` is a top-level
/// expression (the sole expression in its syntactic slot) as opposed to part
/// of a larger expression like `a + NAME()`.
pub(super) fn is_top_level_call(source: &str, call_start: usize, call_end: usize) -> bool {
    let line_start = source[..call_start].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_end = source[call_end..].find('\n').map(|p| call_end + p).unwrap_or(source.len());

    let before = source[line_start..call_start].trim();
    let after = source[call_end..line_end].trim();

    // `call NAME()`
    if before.ends_with("call") && after.is_empty() { return true; }
    // `return NAME()`
    if before.ends_with("return") && after.is_empty() { return true; }
    // `exitwhen NAME()`
    if before.ends_with("exitwhen") && after.is_empty() { return true; }
    // `set VAR = NAME()` / `set VAR[IDX] = NAME()`
    if before.starts_with("set ") && before.ends_with('=') && after.is_empty() { return true; }
    // `if NAME() then` / `elseif NAME() then`
    if before.ends_with("if") && after == "then" { return true; }

    false
}

// ─── Inline substitution ─────────────────────────────────────────────────────

/// Replace `NAME()` calls in `source` with the inlined expression.
///
/// - Top-level calls (sole expression in a `call`/`return`/`set`/`if`/etc.)
///   get the expression as-is.
/// - Nested calls inside larger expressions get the expression wrapped in
///   parentheses when it is compound (binary/unary).
pub(super) fn inline_call_in_source(source: &str, func_name: &str, candidate: &InlineCandidate) -> String {
    let pattern = format!("{}()", func_name);
    let mut result = String::with_capacity(source.len());
    let mut search_from = 0;

    while let Some(pos) = source[search_from..].find(&pattern) {
        let abs_pos = search_from + pos;
        let is_boundary = if abs_pos == 0 {
            true
        } else {
            let b = source.as_bytes()[abs_pos - 1];
            !b.is_ascii_alphanumeric() && b != b'_'
        };

        if !is_boundary {
            result.push_str(&source[search_from..abs_pos + pattern.len()]);
            search_from = abs_pos + pattern.len();
            continue;
        }

        let call_end = abs_pos + pattern.len();
        let top_level = is_top_level_call(source, abs_pos, call_end);

        result.push_str(&source[search_from..abs_pos]);

        if top_level || !candidate.is_compound {
            result.push_str(&candidate.expr_text);
        } else {
            result.push('(');
            result.push_str(&candidate.expr_text);
            result.push(')');
        }

        search_from = call_end;
    }

    result.push_str(&source[search_from..]);
    result
}

// ─── Apply inlines pass ──────────────────────────────────────────────────────

/// Inline functions that take nothing, have a single `return expr` body,
/// and are called exactly once across the entire build output.
///
/// Inlined functions are removed from the function map so they are not
/// emitted in the final output.
pub(super) fn apply_inlines(fragments: &mut Fragments) {
    // Step 1: collect candidates.
    let candidates: HashMap<String, InlineCandidate> = fragments
        .functions
        .iter()
        .filter_map(|(name, frag)| {
            frag.inline_expr.as_ref().map(|ic| (name.clone(), ic.clone()))
        })
        .collect();

    if candidates.is_empty() {
        return;
    }

    // Step 2: count call sites for each candidate across all sources.
    let mut to_inline: Vec<String> = Vec::new();
    for cand_name in candidates.keys() {
        let mut count: usize = 0;
        for (fname, frag) in &fragments.functions {
            if fname == cand_name {
                continue;
            }
            count += count_call_occurrences(&frag.source, cand_name);
        }
        for stmt in &fragments.bare_stmts {
            count += count_call_occurrences(stmt, cand_name);
        }
        for g in &fragments.globals_out {
            count += count_call_occurrences(g, cand_name);
        }
        if count == 1 {
            to_inline.push(cand_name.clone());
        }
    }

    if to_inline.is_empty() {
        return;
    }

    // Step 3: perform replacements.
    for cand_name in &to_inline {
        let candidate = candidates[cand_name].clone();
        for frag in fragments.functions.values_mut() {
            if frag.name == *cand_name {
                continue;
            }
            frag.source = inline_call_in_source(&frag.source, cand_name, &candidate);
            frag.callees.remove(cand_name);
        }
        for stmt in fragments.bare_stmts.iter_mut() {
            *stmt = inline_call_in_source(stmt, cand_name, &candidate);
        }
        for g in fragments.globals_out.iter_mut() {
            *g = inline_call_in_source(g, cand_name, &candidate);
        }
    }

    // Step 4: remove inlined functions.
    for name in &to_inline {
        fragments.functions.remove(name);
    }
}

// ─── StringHash folding ──────────────────────────────────────────────────────

/// Fold `StringHash(expr)` → integer constant in all fragments.
///
/// First collects compile-time constant values (`constant string`, `constant integer`)
/// from globals, then evaluates `StringHash(...)` argument expressions.
/// Also folds string expressions that appear in integer parameter positions.
pub(super) fn fold_string_hash_in_fragments(fragments: &mut Fragments) {
    let constants = collect_constants(&fragments.globals_out);

    // Build signature map: func_name → [param_type, …]
    let signatures = build_signature_map();

    // Pass 1: fold explicit StringHash(...) calls.
    for frag in fragments.functions.values_mut() {
        let folded = fold_string_hash(&frag.source, &constants);
        if folded != frag.source {
            frag.source = folded;
        }
    }
    for stmt in fragments.bare_stmts.iter_mut() {
        let folded = fold_string_hash(stmt, &constants);
        if folded != *stmt {
            *stmt = folded;
        }
    }
    for g in fragments.globals_out.iter_mut() {
        let folded = fold_string_hash(g, &constants);
        if folded != *g {
            *g = folded;
        }
    }

    // Pass 2: fold string arguments in integer parameter positions.
    for frag in fragments.functions.values_mut() {
        let folded = fold_string_integer_args(&frag.source, &constants, &signatures);
        if folded != frag.source {
            frag.source = folded;
        }
    }
    for stmt in fragments.bare_stmts.iter_mut() {
        let folded = fold_string_integer_args(stmt, &constants, &signatures);
        if folded != *stmt {
            *stmt = folded;
        }
    }
}

/// Build a map of `func_name → [param_type, …]` from all known functions/natives.
fn build_signature_map() -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for entry in FILE_STORE.iter() {
        let symbols = &entry.value().file_symbols;
        for f in &symbols.functions {
            let types: Vec<String> = f.params.iter().map(|p| p.type_name.clone()).collect();
            map.insert(f.name.clone(), types);
        }
        for n in &symbols.natives {
            let types: Vec<String> = n.params.iter().map(|p| p.type_name.clone()).collect();
            map.insert(n.name.clone(), types);
        }
    }
    map
}

// ─── Test-only wrappers ──────────────────────────────────────────────────────

/// Test-only: detect an inline candidate from source code.
#[cfg(test)]
pub fn detect_inline_candidate_text(src: &str) -> Option<(String, bool)> {
    use crate::lng::jass::ast::{build_ast, rewrite_imports, Statement};
    use super::convert::convert_function;
    use std::collections::HashSet;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_jass::language().into())
        .ok()?;
    let tree = parser.parse(src, None)?;
    let mut ast = build_ast(tree.root_node());
    let src_bytes = src.as_bytes().to_vec();
    rewrite_imports(&mut ast, &src_bytes);

    for item in &ast.items {
        if let Statement::Function(f) = item {
            if f.params.is_empty() {
                let ir_func = convert_function(src, f, HashSet::new());
                if let Some(ic) = detect_inline_candidate(&ir_func.body) {
                    return Some((ic.expr_text, ic.is_compound));
                }
            }
        }
    }
    None
}

/// Test-only wrapper for [`inline_call_in_source`].
#[cfg(test)]
pub fn inline_call_in_source_text(
    source: &str,
    func_name: &str,
    expr_text: &str,
    is_compound: bool,
) -> String {
    let candidate = InlineCandidate {
        expr_text: expr_text.to_string(),
        is_compound,
    };
    inline_call_in_source(source, func_name, &candidate)
}

/// Test-only wrapper for [`is_top_level_call`].
#[cfg(test)]
pub fn is_top_level_call_text(source: &str, func_name: &str) -> bool {
    let pattern = format!("{}()", func_name);
    if let Some(pos) = source.find(&pattern) {
        is_top_level_call(source, pos, pos + pattern.len())
    } else {
        false
    }
}

