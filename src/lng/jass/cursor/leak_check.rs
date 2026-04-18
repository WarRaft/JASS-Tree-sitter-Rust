//! Handle leak detection and diagnostics for JASS functions.
//!
//! A handle leak occurs when a local variable of a handle type holds a non-null
//! reference when the function exits. JASS does not run destructors on local variables,
//! so the reference count on the underlying handle is never decremented.
//!
//! The analysis tracks a "nullified" set — variables known to be `null` at each point.
//! At every `return` and at the implicit return at `endfunction`, any handle local NOT
//! in the set produces a warning.

use std::collections::HashMap;
use crate::http::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
use crate::http::range::Range;
use crate::lng::jass::ast::{Expr, Statement};
use crate::lng::jass::kind::Kind;
use crate::util::roper::node::NodeExt;
use lapce_xi_rope::Rope;
use tree_sitter::Node;

pub use super::VarInfo;

/// Three-value nullability lattice for handle leak analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullState {
    /// Definitely `null`
    Null,
    /// Definitely not `null`
    NonNull,
    /// Could be either — divergent branches merged
    MaybeNull,
}

impl NullState {
    /// Lattice merge: same→same, different→MaybeNull.
    pub fn join(a: NullState, b: NullState) -> NullState {
        if a == b { a } else { NullState::MaybeNull }
    }
}

/// Per-variable null state map used during flow analysis.
pub type NullMap = HashMap<String, NullState>;

/// Result of inspecting an if-condition for `var == null` / `var != null`.
pub struct NullGuard {
    pub var_name: String,
    pub is_neq: bool,
}

/// A handle-type local variable that needs leak checking.
pub struct HandleLocal {
    pub name: String,
    pub type_name: String,
}

// ─── impl Cursor — handle leak analysis ──────────────────────────────────────

use super::{extract_annotations, Cursor};

impl Cursor {
    // ─── Handle leak detection ────────────────────────────────────────

    /// Check a function body for handle leaks.
    pub(super) fn check_handle_leaks(
        &mut self,
        body: &[Statement],
        func_vars: &HashMap<String, VarInfo>,
        func_node: &Node,
        func_name: &str,
    ) {
        // File-level `//ignore leak` suppresses all handle-leak diagnostics.
        if self.file_ignore_tags.contains("leak") {
            return;
        }

        // 1. Collect handle-type locals (not arrays — arrays don't leak).
        let mut handle_locals: Vec<HandleLocal> = Vec::new();
        for (name, info) in func_vars {
            if info.is_array || info.is_param {
                continue;
            }
            if let Some(ref tn) = info.type_name {
                if Self::is_handle_type(tn) {
                    // Per-variable `//@ignore leak` — check annotation above the local declaration.
                    let local_row = self.find_local_row(body, name);
                    if let Some(row) = local_row {
                        let ann = extract_annotations(&self.rope, row);
                        if ann.ignore_tags.contains("leak") {
                            continue;
                        }
                    }
                    handle_locals.push(HandleLocal {
                        name: name.clone(),
                        type_name: tn.clone(),
                    });
                }
            }
        }

        if handle_locals.is_empty() {
            return;
        }

        // 2. Build initial null state map.
        let mut null_map: NullMap = HashMap::new();
        for hl in &handle_locals {
            null_map.insert(hl.name.clone(), NullState::Null);
        }

        // 3. Walk the body tracking nullability at each exit point.
        let mut top_exits = Vec::new();
        let returned = self.walk_body_for_leaks(
            body,
            &mut null_map,
            &handle_locals,
            &mut top_exits,
            func_name,
        );

        // 4. If the function can fall through (no unconditional return),
        //    check for leaks at the implicit exit (endfunction).
        if !returned {
            let end_range = Self::endfunction_range(func_node, &self.rope);
            for hl in &handle_locals {
                let state = null_map.get(&hl.name).copied().unwrap_or(NullState::Null);
                if state != NullState::Null {
                    self.diagnostics.push(Diagnostic {
                        range: end_range.clone(),
                        message: crate::util::i18n::handle_leak_function_end(
                            &hl.name, &hl.type_name,
                        ),
                        severity: Some(DiagnosticSeverity::Error),
                        source: Some("jass".into()),
                        code: Some(DiagnosticCode::String("leak".into())),
                        data: Some(serde_json::json!({
                            "leak_var": hl.name,
                            "leak_kind": "endfunction",
                            "leak_type": hl.type_name,
                            "func_name": func_name,
                        })),
                        ..Default::default()
                    });
                }
            }
        }
    }

    /// Walk statements tracking nullability state of handle locals.
    /// Returns `true` if every code path ends with a `return`.
    pub(super) fn walk_body_for_leaks(
        &mut self,
        stmts: &[Statement],
        null_map: &mut NullMap,
        handle_locals: &[HandleLocal],
        exit_collector: &mut Vec<NullMap>,
        func_name: &str,
    ) -> bool {
        for stmt in stmts {
            match stmt {
                Statement::Local(l) => {
                    if let Some(name_id) = &l.name {
                        let name = self.node_text(&name_id.node);
                        if handle_locals.iter().any(|hl| hl.name == name) {
                            if let Some(ref val) = l.value {
                                if Self::is_null_expr(val, &self.rope) {
                                    null_map.insert(name, NullState::Null);
                                } else {
                                    null_map.insert(name, NullState::NonNull);
                                }
                            } else {
                                null_map.insert(name, NullState::Null);
                            }
                        }
                    }
                }
                Statement::VarStmt(v) => {
                    for d in &v.decls {
                        if let Some(name_id) = &d.name {
                            let name = self.node_text(&name_id.node);
                            if handle_locals.iter().any(|hl| hl.name == name) {
                                if let Some(ref val) = d.value {
                                    if Self::is_null_expr(val, &self.rope) {
                                        null_map.insert(name, NullState::Null);
                                    } else {
                                        null_map.insert(name, NullState::NonNull);
                                    }
                                } else {
                                    null_map.insert(name, NullState::Null);
                                }
                            }
                        }
                    }
                }
                Statement::Set(s) => {
                    if let Some(var_id) = &s.variable {
                        let name = self.node_text(&var_id.node);
                        if handle_locals.iter().any(|hl| hl.name == name) {
                            if let Some(ref val) = s.value {
                                if Self::is_null_expr(val, &self.rope) {
                                    null_map.insert(name, NullState::Null);
                                } else {
                                    null_map.insert(name, NullState::NonNull);
                                }
                            }
                        }
                    }
                }
                Statement::Exitwhen(_) => {
                    exit_collector.push(null_map.clone());
                }
                Statement::Return(r) => {
                    let returned_local = r.value.as_ref().and_then(|val| {
                        if let Expr::Id(id) = val {
                            let name = id.node.text(&self.rope);
                            handle_locals.iter().find(|hl| hl.name == name)
                        } else {
                            None
                        }
                    });

                    let ret_range = Self::return_keyword_range(&r.node, &self.rope);
                    for hl in handle_locals {
                        let state = null_map.get(&hl.name).copied().unwrap_or(NullState::Null);
                        if state != NullState::Null {
                            let is_returned = returned_local
                                .map(|rl| rl.name == hl.name)
                                .unwrap_or(false);

                            let mut data = serde_json::json!({
                                "leak_var": hl.name,
                                "leak_kind": "return",
                                "leak_type": hl.type_name,
                                "func_name": func_name,
                            });
                            if is_returned {
                                data["returned_local"] = serde_json::json!(true);
                            }

                            self.diagnostics.push(Diagnostic {
                                range: ret_range.clone(),
                                message: crate::util::i18n::handle_leak_before_return(
                                    &hl.name, &hl.type_name,
                                ),
                                severity: Some(DiagnosticSeverity::Error),
                                source: Some("jass".into()),
                                code: Some(DiagnosticCode::String("leak".into())),
                                data: Some(data),
                                ..Default::default()
                            });
                        }
                    }
                    return true;
                }
                Statement::If(i) => {
                    let has_else = i.branches.iter().any(|b| b.condition.is_none());

                    let mut all_return = true;
                    let mut merged: Option<NullMap> = None;
                    let mut returning_guards: Vec<NullGuard> = Vec::new();

                    let mut process_branch = |cond: Option<&Expr>,
                                               body: &[Statement],
                                               null_map: &NullMap,
                                               this: &mut Self|
                     -> (bool, NullMap, Option<NullGuard>) {
                        let mut branch_map = null_map.clone();
                        let guard = cond.and_then(|c| Self::extract_null_guard(c, &this.rope));
                        if let Some(ref g) = guard {
                            if handle_locals.iter().any(|hl| hl.name == g.var_name) {
                                let state = if g.is_neq { NullState::NonNull } else { NullState::Null };
                                branch_map.insert(g.var_name.clone(), state);
                            }
                        }
                        let returned = this.walk_body_for_leaks(
                            body, &mut branch_map, handle_locals, exit_collector, func_name,
                        );
                        (returned, branch_map, guard)
                    };

                    {
                        let (returned, branch_map, guard) =
                            process_branch(i.condition.as_ref(), &i.body, null_map, self);
                        if returned {
                            if let Some(g) = guard { returning_guards.push(g); }
                        } else {
                            all_return = false;
                            merged = Some(branch_map);
                        }
                    }

                    for branch in &i.branches {
                        let (returned, branch_map, guard) =
                            process_branch(branch.condition.as_ref(), &branch.body, null_map, self);
                        if returned {
                            if let Some(g) = guard { returning_guards.push(g); }
                        } else {
                            all_return = false;
                            merged = Some(match merged {
                                Some(acc) => Self::join_null_maps(&acc, &branch_map),
                                None => branch_map,
                            });
                        }
                    }

                    if all_return && has_else {
                        return true;
                    }

                    if !has_else {
                        merged = Some(match merged {
                            Some(acc) => Self::join_null_maps(&acc, null_map),
                            None => null_map.clone(),
                        });
                    }

                    if let Some(ref m) = merged {
                        for (name, state) in m {
                            null_map.insert(name.clone(), *state);
                        }
                    }

                    for guard in &returning_guards {
                        if handle_locals.iter().any(|hl| hl.name == guard.var_name) {
                            let negated = if guard.is_neq {
                                NullState::Null
                            } else {
                                NullState::NonNull
                            };
                            null_map.insert(guard.var_name.clone(), negated);
                        }
                    }
                }
                Statement::Loop(l) => {
                    let mut loop_exits: Vec<NullMap> = Vec::new();
                    let mut loop_map = null_map.clone();
                    let loop_returned = self.walk_body_for_leaks(
                        &l.body,
                        &mut loop_map,
                        handle_locals,
                        &mut loop_exits,
                        func_name,
                    );

                    let mut result = null_map.clone();
                    for exit_map in &loop_exits {
                        result = Self::join_null_maps(&result, exit_map);
                    }
                    if !loop_returned {
                        result = Self::join_null_maps(&result, &loop_map);
                    }
                    for (name, state) in result {
                        null_map.insert(name, state);
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Merge two null maps: for each variable, join their states.
    pub(super) fn join_null_maps(a: &NullMap, b: &NullMap) -> NullMap {
        let mut result = a.clone();
        for (name, b_state) in b {
            let a_state = a.get(name).copied().unwrap_or(NullState::Null);
            result.insert(name.clone(), NullState::join(a_state, *b_state));
        }
        result
    }

    /// Extract the range of only the `return` keyword from a `return_statement` node.
    pub(super) fn return_keyword_range(node: &Node, rope: &Rope) -> Range {
        let count = node.child_count();
        for i in 0..count {
            if let Some(child) = node.child(i as u32) {
                if Kind::try_from(child.grammar_id()) == Ok(Kind::Return) {
                    return child.to_range(rope);
                }
            }
        }
        node.to_range(rope)
    }

    /// Check if an expression is literally `null`.
    pub(super) fn is_null_expr(expr: &Expr, rope: &Rope) -> bool {
        match expr {
            Expr::Id(id) => {
                let text = id.node.text(rope);
                text == "null"
            }
            _ => false,
        }
    }

    /// Try to extract a `var == null` or `var != null` pattern from an expression.
    pub(super) fn extract_null_guard(expr: &Expr, rope: &Rope) -> Option<NullGuard> {
        match expr {
            Expr::Binary { node, left, right } => {
                let op = Self::binary_op_kind(node)?;
                let (var_name, is_neq) = match op {
                    Kind::Neq => {
                        if Self::is_null_expr(right, rope) {
                            if let Expr::Id(id) = left.as_ref() {
                                (id.node.text(rope).to_string(), true)
                            } else {
                                return None;
                            }
                        } else if Self::is_null_expr(left, rope) {
                            if let Expr::Id(id) = right.as_ref() {
                                (id.node.text(rope).to_string(), true)
                            } else {
                                return None;
                            }
                        } else {
                            return None;
                        }
                    }
                    Kind::EqEq => {
                        if Self::is_null_expr(right, rope) {
                            if let Expr::Id(id) = left.as_ref() {
                                (id.node.text(rope).to_string(), false)
                            } else {
                                return None;
                            }
                        } else if Self::is_null_expr(left, rope) {
                            if let Expr::Id(id) = right.as_ref() {
                                (id.node.text(rope).to_string(), false)
                            } else {
                                return None;
                            }
                        } else {
                            return None;
                        }
                    }
                    _ => return None,
                };
                Some(NullGuard { var_name, is_neq })
            }
            Expr::Parens { inner, .. } => Self::extract_null_guard(inner, rope),
            _ => None,
        }
    }


    /// Find the CST row of a local variable declaration in the function body.
    pub(super) fn find_local_row(&self, body: &[Statement], name: &str) -> Option<usize> {
        for stmt in body {
            if let Statement::Local(l) = stmt {
                if let Some(name_id) = &l.name {
                    if self.node_text(&name_id.node) == name {
                        return Some(l.node.start_position().row);
                    }
                }
            }
            if let Statement::VarStmt(v) = stmt {
                for d in &v.decls {
                    if let Some(name_id) = &d.name {
                        if self.node_text(&name_id.node) == name {
                            return Some(v.node.start_position().row);
                        }
                    }
                }
            }
        }
        None
    }

    /// Get the range of the `endfunction` keyword for a FunctionStatement CST node.
    pub(super) fn endfunction_range(func_node: &Node, rope: &Rope) -> Range {
        let count = func_node.child_count();
        for i in (0..count).rev() {
            if let Some(child) = func_node.child(i as u32) {
                if Kind::try_from(child.grammar_id()).ok() == Some(Kind::Endfunction) {
                    return child.to_range(rope);
                }
            }
        }
        func_node.to_range(rope)
    }
}
