//! Render AST nodes back to JASS source text.
//!
//! # Rules
//! - **No direct node slicing for compound nodes.**
//!   Only leaf values are read from `src` by byte offset:
//!   identifier names (`Id`), literals, and operator tokens between sub-expressions.
//! - Every structural node (`FunctionDecl`, `VarStmt`, `IfStmt`, …) is assembled
//!   manually from its typed AST fields.

use crate::lng::jass::ast::{
    CallStmt, Expr, ExitwhenStmt, FunctionCall, FunctionDecl,
    GlobalsBlock, Id, LocalDecl, Param, ReturnStmt,
    SetStmt, Statement, VarStmt,
};
use std::collections::{HashMap, HashSet};

// ─── Leaf helpers (only place src byte-slicing is allowed) ───────────────────

/// Read an identifier's text from the source buffer.
pub fn id_str<'a>(src: &'a str, id: &Id) -> &'a str {
    &src[id.node.start_byte()..id.node.end_byte()]
}

/// Read a literal node's text from the source buffer.
fn lit_str<'a>(src: &'a str, node: &tree_sitter::Node) -> &'a str {
    &src[node.start_byte()..node.end_byte()]
}

/// Extract the operator token that sits between `left_end` and `right_start`
/// in the source (used for `Expr::Binary` and `Expr::Unary`).
fn operator_between(src: &str, left_end: usize, right_start: usize) -> &str {
    src[left_end..right_start].trim()
}

fn indent_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("    {line}")
            }
        })
        .collect()
}

// ─── Expressions ─────────────────────────────────────────────────────────────

pub fn render_expr(src: &str, expr: &Expr) -> String {
    match expr {
        Expr::Id(id) => id_str(src, id).to_string(),
        Expr::Literal(node) => lit_str(src, node).to_string(),
        Expr::FuncRef(id) => format!("function {}", id_str(src, id)),
        Expr::Call(fc) => render_call(src, fc),
        Expr::Parens { inner, .. } => format!("({})", render_expr(src, inner)),
        Expr::Index { array, index, .. } => {
            format!("{}[{}]", render_expr(src, array), render_expr(src, index))
        }
        Expr::Binary { node: _, left, right } => {
            let op = operator_between(src, left.cst_node().end_byte(), right.cst_node().start_byte());
            format!("{} {} {}", render_expr(src, left), op, render_expr(src, right))
        }
        Expr::Unary { node, operand } => {
            let op = operator_between(src, node.start_byte(), operand.cst_node().start_byte());
            format!("{} {}", op, render_expr(src, operand))
        }
    }
}

pub fn render_call(src: &str, call: &FunctionCall) -> String {
    let name = call.name.as_ref().map(|id| id_str(src, id)).unwrap_or("");
    let args: Vec<String> = call.args.iter().map(|a| render_expr(src, a)).collect();
    format!("{}({})", name, args.join(", "))
}

// ─── Parameters ──────────────────────────────────────────────────────────────

fn render_params(src: &str, params: &[Param]) -> String {
    if params.is_empty() {
        return "nothing".to_string();
    }
    params
        .iter()
        .map(|p| {
            let t = p.type_id.as_ref().map(|id| id_str(src, id)).unwrap_or("nothing");
            let n = p.name.as_ref().map(|id| id_str(src, id)).unwrap_or("");
            format!("{t} {n}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ─── Statements ──────────────────────────────────────────────────────────────

pub fn render_local_decl(src: &str, local: &LocalDecl) -> String {
    let mut out = String::from("local ");
    if let Some(t) = &local.type_id {
        out.push_str(id_str(src, t));
        out.push(' ');
    }
    if local.is_array {
        out.push_str("array ");
    }
    if let Some(n) = &local.name {
        out.push_str(id_str(src, n));
    }
    if let Some(v) = &local.value {
        out.push_str(" = ");
        out.push_str(&render_expr(src, v));
    }
    out
}

pub fn render_set_stmt(src: &str, set: &SetStmt) -> String {
    let var = set.variable.as_ref().map(|id| id_str(src, id)).unwrap_or("");
    let mut out = format!("set {var}");
    if let Some(idx) = &set.index {
        out.push('[');
        out.push_str(&render_expr(src, idx));
        out.push(']');
    }
    out.push_str(" = ");
    if let Some(v) = &set.value {
        out.push_str(&render_expr(src, v));
    }
    out
}

pub fn render_call_stmt(src: &str, call: &CallStmt) -> String {
    match &call.func {
        Some(fc) => format!("call {}", render_call(src, fc)),
        None => "call".to_string(),
    }
}

pub fn render_return_stmt(src: &str, ret: &ReturnStmt) -> String {
    match &ret.value {
        Some(v) => format!("return {}", render_expr(src, v)),
        None => "return".to_string(),
    }
}

pub fn render_exitwhen_stmt(src: &str, ew: &ExitwhenStmt) -> String {
    match &ew.condition {
        Some(c) => format!("exitwhen {}", render_expr(src, c)),
        None => "exitwhen".to_string(),
    }
}

// ─── Declarations ─────────────────────────────────────────────────────────────

/// Render a single variable declaration line (used inside `globals` block).
pub fn render_var_stmt(src: &str, var: &VarStmt) -> String {
    let mut out = String::new();
    if var.is_constant {
        out.push_str("constant ");
    }
    if let Some(t) = &var.type_id {
        out.push_str(id_str(src, t));
        out.push(' ');
    }
    if var.is_array {
        out.push_str("array ");
    }
    let decls: Vec<String> = var
        .decls
        .iter()
        .map(|d| {
            let name = d.name.as_ref().map(|id| id_str(src, id)).unwrap_or("");
            match &d.value {
                Some(v) => format!("{name} = {}", render_expr(src, v)),
                None => name.to_string(),
            }
        })
        .collect();
    out.push_str(&decls.join(", "));
    out
}

/// Render `VarStmt` as one or more `local` declarations.
///
/// Used for function-body/bare statements where grammar can produce `VarStmt`
/// without the explicit `local` keyword.
fn render_var_stmt_as_local(src: &str, var: &VarStmt) -> String {
    let type_name = var
        .type_id
        .as_ref()
        .map(|id| id_str(src, id))
        .unwrap_or("integer");

    let mut lines = Vec::new();
    for d in &var.decls {
        let name = d.name.as_ref().map(|id| id_str(src, id)).unwrap_or("");
        let mut line = format!("local {} ", type_name);
        if var.is_array {
            line.push_str("array ");
        }
        line.push_str(name);
        if !var.is_array {
            if let Some(v) = &d.value {
                line.push_str(" = ");
                line.push_str(&render_expr(src, v));
            }
        }
        lines.push(line);
    }
    lines.join("\n")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NullState {
    Null,
    NonNull,
    MaybeNull,
}

impl NullState {
    fn join(a: NullState, b: NullState) -> NullState {
        if a == b {
            a
        } else {
            NullState::MaybeNull
        }
    }
}

type NullMap = HashMap<String, NullState>;

#[derive(Debug, Clone)]
pub(crate) struct HoistRenderState {
    declared_names: HashSet<String>,
    pub(crate) hoisted_local_lines: Vec<String>,
    pub(crate) global_lines: Vec<String>,
    handle_local_types: HashMap<String, String>,
    handle_local_order: Vec<String>,
    null_map: NullMap,
    function_name: Option<String>,
    return_type: Option<String>,
    return_temp_name: Option<String>,
    pub(crate) body_terminated: bool,
}

impl Default for HoistRenderState {
    fn default() -> Self {
        Self {
            declared_names: HashSet::new(),
            hoisted_local_lines: Vec::new(),
            global_lines: Vec::new(),
            handle_local_types: HashMap::new(),
            handle_local_order: Vec::new(),
            null_map: HashMap::new(),
            function_name: None,
            return_type: None,
            return_temp_name: None,
            body_terminated: false,
        }
    }
}

impl HoistRenderState {
    pub(crate) fn for_function(function_name: &str, return_type: &str) -> Self {
        Self {
            function_name: Some(function_name.to_string()),
            return_type: Some(return_type.to_string()),
            ..Self::default()
        }
    }

    fn declare(&mut self, name: &str) -> bool {
        self.declared_names.insert(name.to_string())
    }

    fn ensure_return_temp_local(&mut self) -> Option<String> {
        if let Some(name) = self.return_temp_name.clone() {
            return Some(name);
        }

        let return_type = self.return_type.clone()?;
        if return_type == "nothing" {
            return None;
        }

        let base_stem = self
            .function_name
            .clone()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "ret".to_string());
        let base = format!("{base_stem}_ret");
        let mut candidate = base.clone();
        let mut suffix = 1u32;
        while self.declared_names.contains(&candidate) {
            candidate = format!("{base}{suffix}");
            suffix += 1;
        }
        self.declared_names.insert(candidate.clone());

        let default_val = default_value_for_type(&return_type);
        if is_handle_type(&return_type) {
            // Use a global for handle return temps to avoid local handle leaks.
            self.global_lines.push(format!(
                "{} {} = {}",
                return_type, candidate, default_val
            ));
        } else {
            self.hoisted_local_lines.push(local_decl_line(
                &return_type,
                false,
                &candidate,
                Some(default_val),
            ));
        }
        self.return_temp_name = Some(candidate.clone());
        Some(candidate)
    }
}

fn is_handle_type(type_name: &str) -> bool {
    !matches!(
        type_name,
        "integer" | "real" | "boolean" | "string" | "code" | "nothing" | "null" | "unknown"
    )
}

fn null_state_from_value(src: &str, value: Option<&Expr>) -> NullState {
    match value {
        Some(expr) if is_null_expr(src, expr) => NullState::Null,
        Some(_) => NullState::NonNull,
        None => NullState::Null,
    }
}

fn is_null_expr(src: &str, expr: &Expr) -> bool {
    matches!(expr, Expr::Id(id) if id_str(src, id) == "null")
}

fn register_handle_local(
    state: &mut HoistRenderState,
    null_map: &mut NullMap,
    type_name: &str,
    is_array: bool,
    name: &str,
) {
    if is_array || name.is_empty() || !is_handle_type(type_name) {
        return;
    }

    if !state.handle_local_types.contains_key(name) {
        state
            .handle_local_types
            .insert(name.to_string(), type_name.to_string());
        state.handle_local_order.push(name.to_string());
    }
    null_map.entry(name.to_string()).or_insert(NullState::Null);
}

fn update_handle_set_state(src: &str, null_map: &mut NullMap, name: &str, value: Option<&Expr>) {
    if !null_map.contains_key(name) {
        return;
    }
    if let Some(expr) = value {
        null_map.insert(name.to_string(), null_state_from_value(src, Some(expr)));
    }
}

fn join_null_maps(a: &NullMap, b: &NullMap) -> NullMap {
    let mut result = a.clone();
    for (name, b_state) in b {
        let a_state = a.get(name).copied().unwrap_or(NullState::Null);
        result.insert(name.clone(), NullState::join(a_state, *b_state));
    }
    result
}

fn collect_ids_in_expr(src: &str, expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Id(id) => {
            out.insert(id_str(src, id).to_string());
        }
        Expr::Call(call) => {
            for arg in &call.args {
                collect_ids_in_expr(src, arg, out);
            }
        }
        Expr::FuncRef(_) | Expr::Literal(_) => {}
        Expr::Binary { left, right, .. } => {
            collect_ids_in_expr(src, left, out);
            collect_ids_in_expr(src, right, out);
        }
        Expr::Unary { operand, .. } | Expr::Parens { inner: operand, .. } => {
            collect_ids_in_expr(src, operand, out);
        }
        Expr::Index { array, index, .. } => {
            collect_ids_in_expr(src, array, out);
            collect_ids_in_expr(src, index, out);
        }
    }
}

fn expr_references_live_handle_local(
    src: &str,
    expr: &Expr,
    state: &HoistRenderState,
    null_map: &NullMap,
) -> bool {
    let mut ids = HashSet::new();
    collect_ids_in_expr(src, expr, &mut ids);
    ids.into_iter().any(|name| {
        state.handle_local_types.contains_key(&name)
            && null_map.get(&name).copied().unwrap_or(NullState::Null) != NullState::Null
    })
}

#[derive(Debug, Clone)]
struct NullGuard {
    var_name: String,
    is_neq: bool,
}

fn extract_null_guard(src: &str, expr: &Expr) -> Option<NullGuard> {
    match expr {
        Expr::Binary { left, right, .. } => {
            let op = operator_between(src, left.cst_node().end_byte(), right.cst_node().start_byte());
            let (var_name, is_neq) = match op {
                "!=" => {
                    if is_null_expr(src, right) {
                        if let Expr::Id(id) = left.as_ref() {
                            (id_str(src, id).to_string(), true)
                        } else {
                            return None;
                        }
                    } else if is_null_expr(src, left) {
                        if let Expr::Id(id) = right.as_ref() {
                            (id_str(src, id).to_string(), true)
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                "==" => {
                    if is_null_expr(src, right) {
                        if let Expr::Id(id) = left.as_ref() {
                            (id_str(src, id).to_string(), false)
                        } else {
                            return None;
                        }
                    } else if is_null_expr(src, left) {
                        if let Expr::Id(id) = right.as_ref() {
                            (id_str(src, id).to_string(), false)
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
        Expr::Parens { inner, .. } => extract_null_guard(src, inner),
        _ => None,
    }
}

fn default_value_for_type(type_name: &str) -> &'static str {
    match type_name {
        "integer" | "real" => "0",
        "boolean" => "false",
        "string" => "\"\"",
        _ => "null",
    }
}

fn local_decl_line(type_name: &str, is_array: bool, name: &str, value: Option<&str>) -> String {
    let mut out = format!("local {} ", type_name);
    if is_array {
        out.push_str("array ");
    }
    out.push_str(name);
    if let Some(v) = value {
        out.push_str(" = ");
        out.push_str(v);
    }
    out
}

fn hoist_local_stmt(
    src: &str,
    local: &LocalDecl,
    state: &mut HoistRenderState,
    null_map: &mut NullMap,
    out: &mut Vec<String>,
) {
    let type_name = local
        .type_id
        .as_ref()
        .map(|id| id_str(src, id))
        .unwrap_or("integer");
    let name = local.name.as_ref().map(|id| id_str(src, id)).unwrap_or("");
    if name.is_empty() {
        return;
    }

    if state.declare(name) {
        let init = if local.is_array {
            None
        } else {
            Some(default_value_for_type(type_name))
        };
        state
            .hoisted_local_lines
            .push(local_decl_line(type_name, local.is_array, name, init));
        register_handle_local(state, null_map, type_name, local.is_array, name);
    }

    if !local.is_array {
        if let Some(v) = &local.value {
            out.push(format!("set {} = {}", name, render_expr(src, v)));
            update_handle_set_state(src, null_map, name, Some(v));
        }
    }
}

fn hoist_var_stmt(
    src: &str,
    var: &VarStmt,
    state: &mut HoistRenderState,
    null_map: &mut NullMap,
    out: &mut Vec<String>,
) {
    let type_name = var
        .type_id
        .as_ref()
        .map(|id| id_str(src, id))
        .unwrap_or("integer");

    for decl in &var.decls {
        let name = decl.name.as_ref().map(|id| id_str(src, id)).unwrap_or("");
        if name.is_empty() {
            continue;
        }

        if state.declare(name) {
            let init = if var.is_array {
                None
            } else {
                Some(default_value_for_type(type_name))
            };
            state
                .hoisted_local_lines
                .push(local_decl_line(type_name, var.is_array, name, init));
            register_handle_local(state, null_map, type_name, var.is_array, name);
        }

        if !var.is_array {
            if let Some(v) = &decl.value {
                out.push(format!("set {} = {}", name, render_expr(src, v)));
                update_handle_set_state(src, null_map, name, Some(v));
            }
        }
    }
}

fn render_return_with_leak_fix(
    src: &str,
    ret: &ReturnStmt,
    state: &mut HoistRenderState,
) -> Vec<String> {
    let mut lines = Vec::new();
    let leaking: Vec<String> = state
        .handle_local_order
        .iter()
        .filter(|name| state.null_map.get(*name).copied().unwrap_or(NullState::Null) != NullState::Null)
        .cloned()
        .collect();

    if leaking.is_empty() {
        lines.push(render_return_stmt(src, ret));
        return lines;
    }

    let needs_temp = ret
        .value
        .as_ref()
        .map(|expr| expr_references_live_handle_local(src, expr, state, &state.null_map))
        .unwrap_or(false);

    if needs_temp {
        if let (Some(expr), Some(temp_name)) = (ret.value.as_ref(), state.ensure_return_temp_local()) {
            lines.push(format!("set {} = {}", temp_name, render_expr(src, expr)));
            for name in &leaking {
                lines.push(format!("set {} = null", name));
                state.null_map.insert(name.clone(), NullState::Null);
            }
            lines.push(format!("return {}", temp_name));
            return lines;
        }
    }

    for name in &leaking {
        lines.push(format!("set {} = null", name));
        state.null_map.insert(name.clone(), NullState::Null);
    }
    lines.push(render_return_stmt(src, ret));
    lines
}

fn render_body_impl(
    src: &str,
    stmts: &[Statement],
    state: &mut HoistRenderState,
    null_map: &mut NullMap,
    allow_early_locals: bool,
    exit_collector: &mut Vec<NullMap>,
) -> (Vec<String>, bool) {
    let mut out = Vec::<String>::new();
    let mut seen_executable = false;

    for stmt in stmts {
        match stmt {
            Statement::Comment(_) | Statement::Error(_) => {}
            Statement::Local(local) if allow_early_locals && !seen_executable => {
                let rendered = render_local_decl(src, local);
                for line in rendered.lines() {
                    out.push(line.to_string());
                }
                if let Some(name) = &local.name {
                    let name = id_str(src, name);
                    state.declare(name);
                    let type_name = local
                        .type_id
                        .as_ref()
                        .map(|id| id_str(src, id))
                        .unwrap_or("integer");
                    register_handle_local(state, null_map, type_name, local.is_array, name);
                    if !local.is_array {
                        null_map.insert(name.to_string(), null_state_from_value(src, local.value.as_ref()));
                    }
                }
            }
            Statement::VarStmt(var) if allow_early_locals && !seen_executable => {
                let rendered = render_var_stmt_as_local(src, var);
                for line in rendered.lines() {
                    out.push(line.to_string());
                }
                for decl in &var.decls {
                    if let Some(name) = &decl.name {
                        let name = id_str(src, name);
                        state.declare(name);
                        let type_name = var
                            .type_id
                            .as_ref()
                            .map(|id| id_str(src, id))
                            .unwrap_or("integer");
                        register_handle_local(state, null_map, type_name, var.is_array, name);
                        if !var.is_array {
                            null_map.insert(name.to_string(), null_state_from_value(src, decl.value.as_ref()));
                        }
                    }
                }
            }
            Statement::Local(local) => {
                seen_executable = true;
                hoist_local_stmt(src, local, state, null_map, &mut out);
            }
            Statement::VarStmt(var) => {
                seen_executable = true;
                hoist_var_stmt(src, var, state, null_map, &mut out);
            }
            Statement::If(s) => {
                seen_executable = true;
                let cond = s
                    .condition
                    .as_ref()
                    .map(|c| render_expr(src, c))
                    .unwrap_or_default();
                let mut lines = vec![format!("if {cond} then")];

                let mut all_return = true;
                let mut merged: Option<NullMap> = None;
                let mut returning_guards = Vec::<NullGuard>::new();
                let has_else = s.branches.iter().any(|b| b.condition.is_none());

                let mut first_map = null_map.clone();
                let first_guard = s.condition.as_ref().and_then(|c| extract_null_guard(src, c));
                if let Some(guard) = &first_guard {
                    if state.handle_local_types.contains_key(&guard.var_name) {
                        first_map.insert(
                            guard.var_name.clone(),
                            if guard.is_neq {
                                NullState::NonNull
                            } else {
                                NullState::Null
                            },
                        );
                    }
                }
                let (first_body, first_returned) =
                    render_body_impl(src, &s.body, state, &mut first_map, false, exit_collector);
                lines.extend(indent_lines(&first_body));
                if first_returned {
                    if let Some(guard) = first_guard {
                        returning_guards.push(guard);
                    }
                } else {
                    all_return = false;
                    merged = Some(first_map);
                }

                for branch in &s.branches {
                    match &branch.condition {
                        Some(c) => lines.push(format!("elseif {} then", render_expr(src, c))),
                        None => lines.push("else".to_string()),
                    }

                    let mut branch_map = null_map.clone();
                    let guard = branch.condition.as_ref().and_then(|c| extract_null_guard(src, c));
                    if let Some(guard) = &guard {
                        if state.handle_local_types.contains_key(&guard.var_name) {
                            branch_map.insert(
                                guard.var_name.clone(),
                                if guard.is_neq {
                                    NullState::NonNull
                                } else {
                                    NullState::Null
                                },
                            );
                        }
                    }

                    let (branch_body, branch_returned) = render_body_impl(
                        src,
                        &branch.body,
                        state,
                        &mut branch_map,
                        false,
                        exit_collector,
                    );
                    lines.extend(indent_lines(&branch_body));

                    if branch_returned {
                        if let Some(guard) = guard {
                            returning_guards.push(guard);
                        }
                    } else {
                        all_return = false;
                        merged = Some(match merged {
                            Some(acc) => join_null_maps(&acc, &branch_map),
                            None => branch_map,
                        });
                    }
                }

                lines.push("endif".to_string());
                out.extend(lines);

                if all_return && has_else {
                    return (out, true);
                }

                if !has_else {
                    merged = Some(match merged {
                        Some(acc) => join_null_maps(&acc, null_map),
                        None => null_map.clone(),
                    });
                }

                if let Some(m) = merged {
                    *null_map = m;
                }

                for guard in &returning_guards {
                    if state.handle_local_types.contains_key(&guard.var_name) {
                        null_map.insert(
                            guard.var_name.clone(),
                            if guard.is_neq {
                                NullState::Null
                            } else {
                                NullState::NonNull
                            },
                        );
                    }
                }
            }
            Statement::Loop(s) => {
                seen_executable = true;
                let mut loop_map = null_map.clone();
                let mut loop_exits = Vec::<NullMap>::new();
                let (body, loop_returned) =
                    render_body_impl(src, &s.body, state, &mut loop_map, false, &mut loop_exits);
                out.push("loop".to_string());
                out.extend(indent_lines(&body));
                out.push("endloop".to_string());

                let mut result = null_map.clone();
                for exit_map in &loop_exits {
                    result = join_null_maps(&result, exit_map);
                }
                if !loop_returned {
                    result = join_null_maps(&result, &loop_map);
                }
                *null_map = result;
            }
            Statement::Set(s) => {
                seen_executable = true;
                out.push(render_set_stmt(src, s));
                if s.index.is_none() {
                    if let Some(var) = &s.variable {
                        update_handle_set_state(src, null_map, id_str(src, var), s.value.as_ref());
                    }
                }
            }
            Statement::Call(s) => {
                seen_executable = true;
                out.push(render_call_stmt(src, s));
            }
            Statement::Return(s) => {
                state.null_map = null_map.clone();
                out.extend(render_return_with_leak_fix(src, s, state));
                *null_map = state.null_map.clone();
                return (out, true);
            }
            Statement::Exitwhen(s) => {
                seen_executable = true;
                out.push(render_exitwhen_stmt(src, s));
                exit_collector.push(null_map.clone());
            }
            _ => {
                seen_executable = true;
            }
        }
    }

    (out, false)
}

pub(crate) fn render_body_with_hoisting(
    src: &str,
    stmts: &[Statement],
    state: &mut HoistRenderState,
    allow_early_locals: bool,
) -> Vec<String> {
    if state.body_terminated {
        return Vec::new();
    }

    let mut null_map = state.null_map.clone();
    let mut exit_collector = Vec::<NullMap>::new();
    let (lines, returned) = render_body_impl(
        src,
        stmts,
        state,
        &mut null_map,
        allow_early_locals,
        &mut exit_collector,
    );
    state.null_map = null_map;
    if returned {
        state.body_terminated = true;
    }
    lines
}

pub(crate) fn render_body_epilogue(state: &HoistRenderState) -> Vec<String> {
    if state.body_terminated {
        return Vec::new();
    }

    state
        .handle_local_order
        .iter()
        .filter_map(|name| {
            if state.null_map.get(name).copied().unwrap_or(NullState::Null) != NullState::Null {
                Some(format!("set {} = null", name))
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn render_main_from_statements(src: &str, stmts: &[Statement]) -> String {
    let mut state = HoistRenderState::default();
    let body = render_body_with_hoisting(src, stmts, &mut state, false);
    let epilogue = render_body_epilogue(&state);

    let mut out = String::from("function main takes nothing returns nothing\n");
    for line in state
        .hoisted_local_lines
        .iter()
        .chain(body.iter())
        .chain(epilogue.iter())
    {
        if line.trim().is_empty() {
            continue;
        }
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("endfunction");
    out
}

/// Return only the inner variable lines for a `globals` block.
/// The caller assembles the `globals` / `endglobals` wrapper.
pub fn render_globals_vars(src: &str, globals: &GlobalsBlock) -> Vec<String> {
    globals.vars.iter().map(|v| render_var_stmt(src, v)).collect()
}

/// Render a complete `function … endfunction` block.
/// Returns `(function_text, extra_globals)` where `extra_globals` are variable
/// declaration lines that must be placed in a `globals` block (e.g. handle
/// return-temp variables for leak-safe returns).
pub fn render_function(src: &str, func: &FunctionDecl) -> (String, Vec<String>) {
    let prefix = if func.is_constant { "constant function " } else { "function " };
    let name = func.name.as_ref().map(|id| id_str(src, id)).unwrap_or("");
    let params = render_params(src, &func.params);
    let ret = func
        .return_type
        .as_ref()
        .map(|id| id_str(src, id))
        .unwrap_or("nothing");

    let mut out = format!("{prefix}{name} takes {params} returns {ret}\n");

    let mut state = HoistRenderState::for_function(name, ret);
    // Pre-populate declared names with parameter names so the return-temp
    // generator cannot collide with any parameter.
    for p in &func.params {
        if let Some(id) = &p.name {
            state.declare(id_str(src, id));
        }
    }
    let body_lines = render_body_with_hoisting(src, &func.body, &mut state, true);
    let epilogue = render_body_epilogue(&state);
    for line in state
        .hoisted_local_lines
        .iter()
        .chain(body_lines.iter())
        .chain(epilogue.iter())
    {
        if line.trim().is_empty() {
            continue;
        }
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }

    out.push_str("endfunction");
    (out, state.global_lines)
}


#[cfg(test)]
#[path = "render_test.rs"]
mod render_test;

