//! IR pass: insert `set <var> = null` before exit points for handle-type locals.
//!
//! This replicates the diagnostic-driven handle-leak detection but instead of
//! emitting warnings, it **inserts** the necessary `set <var> = null` statements
//! directly into the IR before every `return` and at the implicit function exit.
//!
//! The analysis is the same as in `cursor.rs`:
//! - Collect handle-type locals (not arrays, not params).
//! - Walk the body tracking a nullability map (`Null` / `NonNull` / `MaybeNull`).
//! - Before each `return` and at `endfunction`, insert `set <var> = null`
//!   for every handle local that is **not** definitely `Null`.
//!
//! This pass must run **after** `hoist_ir_locals` (all locals at the top)
//! and **before** `fold_ir` / `uglify_ir`.

use super::ir::*;
use std::collections::{HashMap, HashSet};

// ─── Nullability lattice (mirrors cursor.rs) ─────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NullState {
    Null,
    NonNull,
    MaybeNull,
}

impl NullState {
    fn join(a: NullState, b: NullState) -> NullState {
        if a == b { a } else { NullState::MaybeNull }
    }
}

type NullMap = HashMap<String, NullState>;

// ─── Handle-type predicate ───────────────────────────────────────────────────

fn is_handle_type(type_name: &str) -> bool {
    !matches!(
        type_name,
        "integer" | "real" | "boolean" | "string" | "code" | "nothing" | "null" | "unknown"
    )
}

// ─── Expression analysis ─────────────────────────────────────────────────────

/// Check if an IR expression is the literal `null`.
fn is_null_literal(expr: &IRExpr) -> bool {
    matches!(expr, IRExpr::Literal(s) if s == "null" || s == "nil")
}

/// Try to extract a `var == null` / `var != null` guard from a condition.
fn extract_null_guard(expr: &IRExpr) -> Option<(String, bool)> {
    match expr {
        IRExpr::Binary { left, op, right } => {
            let is_neq = op == "!=" || op == "neq";
            let is_eq = op == "==" || op == "eqeq";
            if !is_neq && !is_eq {
                return None;
            }
            // var OP null
            if is_null_literal(right) {
                if let IRExpr::Id(name) = left.as_ref() {
                    return Some((name.clone(), is_neq));
                }
            }
            // null OP var
            if is_null_literal(left) {
                if let IRExpr::Id(name) = right.as_ref() {
                    return Some((name.clone(), is_neq));
                }
            }
            None
        }
        IRExpr::Parens(inner) => extract_null_guard(inner),
        _ => None,
    }
}

// ─── Null-state tracking walk ────────────────────────────────────────────────

/// Walk statements, tracking null-state.  Returns `true` if every path ends
/// with `return`.
///
/// `exit_collector` accumulates null-map snapshots at each `exitwhen`.
fn walk_body(
    stmts: &[IRStmt],
    null_map: &mut NullMap,
    handle_locals: &[String],
    exit_collector: &mut Vec<NullMap>,
) -> bool {
    for stmt in stmts {
        match stmt {
            IRStmt::Local { name, value, .. } => {
                if handle_locals.contains(name) {
                    if let Some(v) = value {
                        if is_null_literal(v) {
                            null_map.insert(name.clone(), NullState::Null);
                        } else {
                            null_map.insert(name.clone(), NullState::NonNull);
                        }
                    } else {
                        null_map.insert(name.clone(), NullState::Null);
                    }
                }
            }
            IRStmt::Set { var, value, index: None } => {
                if handle_locals.contains(var) {
                    if is_null_literal(value) {
                        null_map.insert(var.clone(), NullState::Null);
                    } else {
                        null_map.insert(var.clone(), NullState::NonNull);
                    }
                }
            }
            IRStmt::Call { .. } | IRStmt::Set { .. } | IRStmt::VarDecl { .. } => {}
            IRStmt::Exitwhen(_) => {
                exit_collector.push(null_map.clone());
            }
            IRStmt::Return(_) => {
                return true;
            }
            IRStmt::If { condition, body, branches } => {
                let has_else = branches.iter().any(|b| b.condition.is_none());
                let mut all_return = true;
                let mut merged: Option<NullMap> = None;
                let mut returning_guards: Vec<(String, bool)> = Vec::new();

                // Process if-body.
                {
                    let mut branch_map = null_map.clone();
                    if let Some((var, is_neq)) = extract_null_guard(condition) {
                        if handle_locals.contains(&var) {
                            let state = if is_neq { NullState::NonNull } else { NullState::Null };
                            branch_map.insert(var.clone(), state);
                        }
                    }
                    let guard = extract_null_guard(condition);
                    let returned = walk_body(body, &mut branch_map, handle_locals, exit_collector);
                    if returned {
                        if let Some(g) = guard { returning_guards.push(g); }
                    } else {
                        all_return = false;
                        merged = Some(branch_map);
                    }
                }

                // Process elseif / else branches.
                for branch in branches {
                    let mut branch_map = null_map.clone();
                    if let Some(ref cond) = branch.condition {
                        if let Some((var, is_neq)) = extract_null_guard(cond) {
                            if handle_locals.contains(&var) {
                                let state = if is_neq { NullState::NonNull } else { NullState::Null };
                                branch_map.insert(var.clone(), state);
                            }
                        }
                    }
                    let guard = branch.condition.as_ref().and_then(extract_null_guard);
                    let returned = walk_body(&branch.body, &mut branch_map, handle_locals, exit_collector);
                    if returned {
                        if let Some(g) = guard { returning_guards.push(g); }
                    } else {
                        all_return = false;
                        merged = Some(match merged {
                            Some(acc) => join_maps(&acc, &branch_map),
                            None => branch_map,
                        });
                    }
                }

                if all_return && has_else {
                    return true;
                }

                if !has_else {
                    merged = Some(match merged {
                        Some(acc) => join_maps(&acc, null_map),
                        None => null_map.clone(),
                    });
                }

                if let Some(ref m) = merged {
                    for (name, state) in m {
                        null_map.insert(name.clone(), *state);
                    }
                }

                for (var, is_neq) in &returning_guards {
                    if handle_locals.contains(var) {
                        let negated = if *is_neq { NullState::Null } else { NullState::NonNull };
                        null_map.insert(var.clone(), negated);
                    }
                }
            }
            IRStmt::Loop(body) => {
                let mut loop_exits: Vec<NullMap> = Vec::new();
                let mut loop_map = null_map.clone();
                let loop_returned = walk_body(body, &mut loop_map, handle_locals, &mut loop_exits);

                let mut result = null_map.clone();
                for exit_map in &loop_exits {
                    result = join_maps(&result, exit_map);
                }
                if !loop_returned {
                    result = join_maps(&result, &loop_map);
                }
                for (name, state) in result {
                    null_map.insert(name, state);
                }
            }
        }
    }
    false
}

fn join_maps(a: &NullMap, b: &NullMap) -> NullMap {
    let mut result = a.clone();
    for (name, b_state) in b {
        let a_state = a.get(name).copied().unwrap_or(NullState::Null);
        result.insert(name.clone(), NullState::join(a_state, *b_state));
    }
    result
}

// ─── Leak-fix insertion ──────────────────────────────────────────────────────

/// Generate `set <var> = null` statements for all leaking handle locals,
/// **excluding** the variable named `skip` (if any).
fn null_sets_for_leaks(
    null_map: &NullMap,
    handle_locals: &[String],
    skip: Option<&str>,
) -> Vec<IRStmt> {
    let mut stmts = Vec::new();
    for name in handle_locals {
        if skip == Some(name.as_str()) {
            continue;
        }
        let state = null_map.get(name).copied().unwrap_or(NullState::Null);
        if state != NullState::Null {
            stmts.push(IRStmt::Set {
                var: name.clone(),
                index: None,
                value: IRExpr::null(),
            });
        }
    }
    stmts
}

/// Context passed through `insert_fixes_in_body` for the returned-local
/// temp-global transformation.
struct LeakFixCtx {
    /// `local_name → type_name` for handle-type locals.
    local_types: HashMap<String, String>,
    /// `type_name → temp_global_name` assigned by `fix_leaks`.
    temp_globals: HashMap<String, String>,
}

impl LeakFixCtx {
    /// If the return value is `IRExpr::Id(name)` referencing a handle local
    /// that has a temp global, return `(local_name, temp_global_name)`.
    fn returned_local_info(&self, val: &Option<IRExpr>) -> Option<(String, String)> {
        if let Some(IRExpr::Id(name)) = val {
            if let Some(type_name) = self.local_types.get(name) {
                if let Some(temp) = self.temp_globals.get(type_name) {
                    return Some((name.clone(), temp.clone()));
                }
            }
        }
        None
    }
}

/// Recursively insert leak fixes before every `return` in a statement list.
/// Returns a new list of statements with fixes inserted.
fn insert_fixes_in_body(
    stmts: Vec<IRStmt>,
    null_map: &mut NullMap,
    handle_locals: &[String],
    ctx: &LeakFixCtx,
) -> Vec<IRStmt> {
    let mut result = Vec::with_capacity(stmts.len());

    for stmt in stmts {
        match stmt {
            IRStmt::Local { ref name, ref value, .. } => {
                if handle_locals.contains(name) {
                    if let Some(v) = value {
                        if is_null_literal(v) {
                            null_map.insert(name.clone(), NullState::Null);
                        } else {
                            null_map.insert(name.clone(), NullState::NonNull);
                        }
                    } else {
                        null_map.insert(name.clone(), NullState::Null);
                    }
                }
                result.push(stmt);
            }
            IRStmt::Set { ref var, ref value, index: None } if handle_locals.contains(var) => {
                if is_null_literal(value) {
                    null_map.insert(var.clone(), NullState::Null);
                } else {
                    null_map.insert(var.clone(), NullState::NonNull);
                }
                result.push(stmt);
            }
            IRStmt::Return(ref val) => {
                // Check if the return value is a handle-type local variable.
                if let Some((local_name, temp_global)) = ctx.returned_local_info(val) {
                    let state = null_map.get(&local_name).copied().unwrap_or(NullState::Null);
                    if state != NullState::Null {
                        // set temp_global = localvar
                        result.push(IRStmt::Set {
                            var: temp_global.clone(),
                            index: None,
                            value: IRExpr::Id(local_name.clone()),
                        });
                        // null-sets for all OTHER leaking locals
                        result.extend(null_sets_for_leaks(null_map, handle_locals, Some(&local_name)));
                        // set localvar = null
                        result.push(IRStmt::Set {
                            var: local_name,
                            index: None,
                            value: IRExpr::null(),
                        });
                        // return temp_global
                        result.push(IRStmt::Return(Some(IRExpr::Id(temp_global))));
                        return result;
                    }
                }
                // Normal case: insert null-sets before the return.
                result.extend(null_sets_for_leaks(null_map, handle_locals, None));
                result.push(stmt);
                return result; // everything after a return is dead
            }
            IRStmt::If { condition, body, branches } => {
                let has_else = branches.iter().any(|b| b.condition.is_none());
                let mut all_return = true;
                let mut merged: Option<NullMap> = None;
                let mut returning_guards: Vec<(String, bool)> = Vec::new();

                // Process if-body.
                let new_if_body;
                {
                    let mut branch_map = null_map.clone();
                    let guard = extract_null_guard(&condition);
                    if let Some((ref var, is_neq)) = guard {
                        if handle_locals.contains(var) {
                            let state = if is_neq { NullState::NonNull } else { NullState::Null };
                            branch_map.insert(var.clone(), state);
                        }
                    }
                    new_if_body = insert_fixes_in_body(body, &mut branch_map, handle_locals, ctx);
                    let returned = body_returns(&new_if_body);
                    if returned {
                        if let Some(g) = guard { returning_guards.push(g); }
                    } else {
                        all_return = false;
                        merged = Some(branch_map);
                    }
                }

                // Process elseif / else branches.
                let mut new_branches = Vec::with_capacity(branches.len());
                for branch in branches {
                    let mut branch_map = null_map.clone();
                    let guard = branch.condition.as_ref().and_then(extract_null_guard);
                    if let Some((ref var, is_neq)) = guard {
                        if handle_locals.contains(var) {
                            let state = if is_neq { NullState::NonNull } else { NullState::Null };
                            branch_map.insert(var.clone(), state);
                        }
                    }
                    let new_body = insert_fixes_in_body(branch.body, &mut branch_map, handle_locals, ctx);
                    let returned = body_returns(&new_body);
                    if returned {
                        if let Some(g) = guard { returning_guards.push(g); }
                    } else {
                        all_return = false;
                        merged = Some(match merged {
                            Some(acc) => join_maps(&acc, &branch_map),
                            None => branch_map,
                        });
                    }
                    new_branches.push(IRBranch {
                        condition: branch.condition,
                        body: new_body,
                    });
                }

                if !has_else {
                    merged = Some(match merged {
                        Some(acc) => join_maps(&acc, null_map),
                        None => null_map.clone(),
                    });
                }

                if let Some(ref m) = merged {
                    for (name, state) in m {
                        null_map.insert(name.clone(), *state);
                    }
                }

                for (var, is_neq) in &returning_guards {
                    if handle_locals.contains(var) {
                        let negated = if *is_neq { NullState::Null } else { NullState::NonNull };
                        null_map.insert(var.clone(), negated);
                    }
                }

                result.push(IRStmt::If {
                    condition,
                    body: new_if_body,
                    branches: new_branches,
                });

                if all_return && has_else {
                    return result;
                }
            }
            IRStmt::Loop(body) => {
                // First pass: track null-state to know the post-loop state.
                let mut loop_exits: Vec<NullMap> = Vec::new();
                let mut track_map = null_map.clone();
                let _loop_returned = walk_body(&body, &mut track_map, handle_locals, &mut loop_exits);

                // Second pass: insert fixes inside the loop body.
                let mut inner_map = null_map.clone();
                let new_body = insert_fixes_in_body(body, &mut inner_map, handle_locals, ctx);

                // Update post-loop state.
                let mut post = null_map.clone();
                for exit_map in &loop_exits {
                    post = join_maps(&post, exit_map);
                }
                if !body_returns(&new_body) {
                    post = join_maps(&post, &inner_map);
                }
                for (name, state) in post {
                    null_map.insert(name, state);
                }

                result.push(IRStmt::Loop(new_body));
            }
            IRStmt::Exitwhen(_) => {
                // exitwhen doesn't change null state of locals.
                result.push(stmt);
            }
            other => {
                result.push(other);
            }
        }
    }

    result
}

/// Check if a statement list's last effective statement is a `return`.
fn body_returns(stmts: &[IRStmt]) -> bool {
    for stmt in stmts.iter().rev() {
        match stmt {
            IRStmt::Return(_) => return true,
            IRStmt::If { branches, body, .. } => {
                let has_else = branches.iter().any(|b| b.condition.is_none());
                if has_else && body_returns(body) && branches.iter().all(|b| body_returns(&b.body)) {
                    return true;
                }
                return false;
            }
            _ => return false,
        }
    }
    false
}

// ─── Per-function driver ─────────────────────────────────────────────────────

/// Fix handle leaks in a single function's body.
///
/// `temp_globals` maps `type_name → global_variable_name` for the
/// returned-local transformation.
fn fix_function_leaks(func: &mut IRFunc, temp_globals: &HashMap<String, String>) {
    // 1. Collect handle-type locals (after hoisting, all are at the top).
    let mut handle_locals: Vec<String> = Vec::new();
    let mut local_types: HashMap<String, String> = HashMap::new();
    for stmt in &func.body {
        if let IRStmt::Local { type_name, is_array, name, .. } = stmt {
            if !is_array && is_handle_type(type_name) {
                handle_locals.push(name.clone());
                local_types.insert(name.clone(), type_name.clone());
            }
        }
    }

    if handle_locals.is_empty() {
        return;
    }

    let ctx = LeakFixCtx {
        local_types,
        temp_globals: temp_globals.clone(),
    };

    // 2. Build initial null state map — all handle locals start as Null.
    let mut null_map: NullMap = HashMap::new();
    for name in &handle_locals {
        null_map.insert(name.clone(), NullState::Null);
    }

    // 3. Insert fixes and track null-state through the body.
    let body = std::mem::take(&mut func.body);
    let mut fixed_body = insert_fixes_in_body(body, &mut null_map, &handle_locals, &ctx);

    // 4. If the function can fall through (no unconditional return at the end),
    //    append null-sets for any leaking locals before the implicit exit.
    if !body_returns(&fixed_body) {
        fixed_body.extend(null_sets_for_leaks(&null_map, &handle_locals, None));
    }

    func.body = fixed_body;
}

// ─── Returned-local type scanning ────────────────────────────────────────────

/// Recursively scan a function body for `return <handle_local>` patterns
/// and collect the types that need a temp global variable.
fn collect_returned_local_types(
    stmts: &[IRStmt],
    handle_locals: &HashMap<String, String>, // name → type_name
    needed: &mut HashSet<String>,            // type_names
) {
    for stmt in stmts {
        match stmt {
            IRStmt::Return(Some(IRExpr::Id(name))) => {
                if let Some(type_name) = handle_locals.get(name) {
                    needed.insert(type_name.clone());
                }
            }
            IRStmt::If { body, branches, .. } => {
                collect_returned_local_types(body, handle_locals, needed);
                for b in branches {
                    collect_returned_local_types(&b.body, handle_locals, needed);
                }
            }
            IRStmt::Loop(body) => {
                collect_returned_local_types(body, handle_locals, needed);
            }
            _ => {}
        }
    }
}

// ─── Public entry point ──────────────────────────────────────────────────────

/// Insert `set <var> = null` for all handle-type locals that would leak
/// at function exit points.
///
/// When a `return` expression references a handle-type local, a single
/// temp global per type is created and the return is rewritten:
///   `set <temp> = <local>`
///   `set <local> = null`
///   `return <temp>`
///
/// Call after `hoist_ir_locals` and before `fold_ir` / `uglify_ir`.
pub(super) fn fix_leaks(ir: &mut BuildIR) {
    // 1. Scan all functions to discover which handle types need a temp global.
    let mut needed_types: HashSet<String> = HashSet::new();
    for func in ir.functions.values() {
        let mut handle_locals: HashMap<String, String> = HashMap::new();
        for stmt in &func.body {
            if let IRStmt::Local { type_name, is_array, name, .. } = stmt {
                if !is_array && is_handle_type(type_name) {
                    handle_locals.insert(name.clone(), type_name.clone());
                }
            }
        }
        if !handle_locals.is_empty() {
            collect_returned_local_types(&func.body, &handle_locals, &mut needed_types);
        }
    }

    // 2. Collect all existing global names to avoid collisions.
    let mut all_names: HashSet<String> = HashSet::new();
    for g in &ir.globals {
        if let IRStmt::VarDecl { decls, .. } = g {
            for d in decls {
                all_names.insert(d.name.clone());
            }
        }
    }
    for name in ir.functions.keys() {
        all_names.insert(name.clone());
    }

    // 3. Create one temp global per needed type.
    //    Name: `_lr_<type>`, with numeric suffix for uniqueness.
    let mut temp_globals: HashMap<String, String> = HashMap::new();
    for type_name in &needed_types {
        let base = format!("_lr_{}", type_name);
        let mut candidate = base.clone();
        let mut suffix = 1u32;
        while all_names.contains(&candidate) {
            candidate = format!("{}{}", base, suffix);
            suffix += 1;
        }
        all_names.insert(candidate.clone());
        temp_globals.insert(type_name.clone(), candidate);
    }

    // 4. Fix leaks in every function.
    for func in ir.functions.values_mut() {
        fix_function_leaks(func, &temp_globals);
    }

    // 5. Emit temp global declarations into ir.globals.
    for (type_name, var_name) in &temp_globals {
        ir.globals.push(IRStmt::VarDecl {
            is_constant: false,
            is_array: false,
            type_name: type_name.clone(),
            decls: vec![IRVarInit {
                name: var_name.clone(),
                short_name: None,
                value: None,
            }],
        });
    }
}

