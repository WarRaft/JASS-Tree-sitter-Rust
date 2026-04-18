//! Render AST nodes back to JASS source text.
//!
//! # Rules
//! - **No direct node slicing for compound nodes.**
//!   Only leaf values are read from `src` by byte offset:
//!   identifier names (`Id`), literals, and operator tokens between sub-expressions.
//! - Every structural node (`FunctionDecl`, `VarStmt`, `IfStmt`, …) is assembled
//!   manually from its typed AST fields.

#![allow(dead_code)]

use crate::lng::jass::ast::{
    CallStmt, Expr, ExitwhenStmt, FunctionCall, FunctionDecl,
    GlobalsBlock, Id, LocalDecl, Param, ReturnStmt,
    SetStmt, Statement, VarStmt,
};
use std::collections::{HashMap, HashSet};

type RenameMap = HashMap<String, String>;

fn merge_rename_maps(base: &RenameMap, overlay: &RenameMap) -> RenameMap {
    let mut merged = base.clone();
    merged.extend(overlay.clone());
    merged
}

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

fn renamed_name(src: &str, id: &Id, renames: &RenameMap) -> String {
    let name = id_str(src, id);
    renames
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.to_string())
}

fn renamed_opt_name(src: &str, id: Option<&Id>, renames: &RenameMap) -> String {
    id.map(|id| renamed_name(src, id, renames)).unwrap_or_default()
}

fn collect_function_scope_decl_names(src: &str, stmts: &[Statement], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Statement::Local(local) => {
                if let Some(id) = &local.name {
                    out.insert(id_str(src, id).to_string());
                }
            }
            Statement::VarStmt(var) => {
                for decl in &var.decls {
                    if let Some(id) = &decl.name {
                        out.insert(id_str(src, id).to_string());
                    }
                }
            }
            Statement::If(s) => {
                collect_function_scope_decl_names(src, &s.body, out);
                for branch in &s.branches {
                    collect_function_scope_decl_names(src, &branch.body, out);
                }
            }
            Statement::Loop(s) => collect_function_scope_decl_names(src, &s.body, out),
            _ => {}
        }
    }
}

fn mint_renamed_ident(base: &str, used_names: &mut HashSet<String>) -> String {
    let mut suffix = 1u32;
    loop {
        let candidate = format!("{base}{suffix}");
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn build_function_rename_map(
    src: &str,
    func: &FunctionDecl,
    function_names: &HashSet<String>,
) -> RenameMap {
    let mut used_names = function_names.clone();

    for param in &func.params {
        if let Some(id) = &param.name {
            used_names.insert(id_str(src, id).to_string());
        }
    }
    collect_function_scope_decl_names(src, &func.body, &mut used_names);

    let mut renames = HashMap::new();
    for param in &func.params {
        if let Some(id) = &param.name {
            let name = id_str(src, id);
            if function_names.contains(name) && !renames.contains_key(name) {
                let renamed = mint_renamed_ident(name, &mut used_names);
                renames.insert(name.to_string(), renamed);
            }
        }
    }

    fn collect_stmt_renames(
        src: &str,
        stmts: &[Statement],
        function_names: &HashSet<String>,
        used_names: &mut HashSet<String>,
        renames: &mut RenameMap,
    ) {
        for stmt in stmts {
            match stmt {
                Statement::Local(local) => {
                    if let Some(id) = &local.name {
                        let name = id_str(src, id);
                        if function_names.contains(name) && !renames.contains_key(name) {
                            let renamed = mint_renamed_ident(name, used_names);
                                renames.insert(name.to_string(), renamed);
                        }
                    }
                }
                Statement::VarStmt(var) => {
                    for decl in &var.decls {
                        if let Some(id) = &decl.name {
                            let name = id_str(src, id);
                            if function_names.contains(name) && !renames.contains_key(name) {
                                let renamed = mint_renamed_ident(name, used_names);
                                renames.insert(name.to_string(), renamed);
                            }
                        }
                    }
                }
                Statement::If(s) => {
                    collect_stmt_renames(src, &s.body, function_names, used_names, renames);
                    for branch in &s.branches {
                        collect_stmt_renames(src, &branch.body, function_names, used_names, renames);
                    }
                }
                Statement::Loop(s) => {
                    collect_stmt_renames(src, &s.body, function_names, used_names, renames)
                }
                _ => {}
            }
        }
    }

    collect_stmt_renames(src, &func.body, function_names, &mut used_names, &mut renames);
    renames
}

// ─── Expressions ─────────────────────────────────────────────────────────────

pub fn render_expr(src: &str, expr: &Expr) -> String {
    render_expr_with_renames(src, expr, &HashMap::new())
}

fn render_expr_with_renames(src: &str, expr: &Expr, renames: &RenameMap) -> String {
    match expr {
        Expr::Id(id) => renamed_name(src, id, renames),
        Expr::Literal(node) => lit_str(src, node).to_string(),
        Expr::FuncRef(id) => format!("function {}", id_str(src, id)),
        Expr::Call(fc) => render_call_with_renames(src, fc, renames),
        Expr::Parens { inner, .. } => format!("({})", render_expr_with_renames(src, inner, renames)),
        Expr::Index { array, index, .. } => {
            format!(
                "{}[{}]",
                render_expr_with_renames(src, array, renames),
                render_expr_with_renames(src, index, renames)
            )
        }
        Expr::Binary { node: _, left, right } => {
            let op = operator_between(src, left.cst_node().end_byte(), right.cst_node().start_byte());
            format!(
                "{} {} {}",
                render_expr_with_renames(src, left, renames),
                op,
                render_expr_with_renames(src, right, renames)
            )
        }
        Expr::Unary { node, operand } => {
            let op = operator_between(src, node.start_byte(), operand.cst_node().start_byte());
            format!("{} {}", op, render_expr_with_renames(src, operand, renames))
        }
    }
}

pub fn render_call(src: &str, call: &FunctionCall) -> String {
    render_call_with_renames(src, call, &HashMap::new())
}

fn render_call_with_renames(src: &str, call: &FunctionCall, renames: &RenameMap) -> String {
    let name = call.name.as_ref().map(|id| id_str(src, id)).unwrap_or("");
    let args: Vec<String> = call
        .args
        .iter()
        .map(|a| render_expr_with_renames(src, a, renames))
        .collect();
    format!("{}({})", name, args.join(", "))
}

// ─── Parameters ──────────────────────────────────────────────────────────────

fn render_params(src: &str, params: &[Param]) -> String {
    render_params_with_renames(src, params, &HashMap::new())
}

fn render_params_with_renames(src: &str, params: &[Param], renames: &RenameMap) -> String {
    if params.is_empty() {
        return "nothing".to_string();
    }
    params
        .iter()
        .map(|p| {
            let t = p.type_id.as_ref().map(|id| id_str(src, id)).unwrap_or("nothing");
            let n = renamed_opt_name(src, p.name.as_ref(), renames);
            format!("{t} {n}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ─── Statements ──────────────────────────────────────────────────────────────

pub fn render_local_decl(src: &str, local: &LocalDecl) -> String {
    render_local_decl_with_renames(src, local, &HashMap::new())
}

fn render_local_decl_with_renames(src: &str, local: &LocalDecl, renames: &RenameMap) -> String {
    let mut out = String::from("local ");
    if let Some(t) = &local.type_id {
        out.push_str(id_str(src, t));
        out.push(' ');
    }
    if local.is_array {
        out.push_str("array ");
    }
    if let Some(n) = &local.name {
        out.push_str(&renamed_name(src, n, renames));
    }
    if let Some(v) = &local.value {
        out.push_str(" = ");
        out.push_str(&render_expr_with_renames(src, v, renames));
    }
    out
}

pub fn render_set_stmt(src: &str, set: &SetStmt) -> String {
    render_set_stmt_with_renames(src, set, &HashMap::new())
}

fn render_set_stmt_with_renames(src: &str, set: &SetStmt, renames: &RenameMap) -> String {
    let var = renamed_opt_name(src, set.variable.as_ref(), renames);
    let mut out = format!("set {var}");
    if let Some(idx) = &set.index {
        out.push('[');
        out.push_str(&render_expr_with_renames(src, idx, renames));
        out.push(']');
    }
    out.push_str(" = ");
    if let Some(v) = &set.value {
        out.push_str(&render_expr_with_renames(src, v, renames));
    }
    out
}

pub fn render_call_stmt(src: &str, call: &CallStmt) -> String {
    render_call_stmt_with_renames(src, call, &HashMap::new())
}

fn render_call_stmt_with_renames(src: &str, call: &CallStmt, renames: &RenameMap) -> String {
    match &call.func {
        Some(fc) => format!("call {}", render_call_with_renames(src, fc, renames)),
        None => "call".to_string(),
    }
}

pub fn render_return_stmt(src: &str, ret: &ReturnStmt) -> String {
    render_return_stmt_with_renames(src, ret, &HashMap::new())
}

fn render_return_stmt_with_renames(src: &str, ret: &ReturnStmt, renames: &RenameMap) -> String {
    match &ret.value {
        Some(v) => format!("return {}", render_expr_with_renames(src, v, renames)),
        None => "return".to_string(),
    }
}

pub fn render_exitwhen_stmt(src: &str, ew: &ExitwhenStmt) -> String {
    render_exitwhen_stmt_with_renames(src, ew, &HashMap::new())
}

fn render_exitwhen_stmt_with_renames(src: &str, ew: &ExitwhenStmt, renames: &RenameMap) -> String {
    match &ew.condition {
        Some(c) => format!("exitwhen {}", render_expr_with_renames(src, c, renames)),
        None => "exitwhen".to_string(),
    }
}

// ─── Declarations ─────────────────────────────────────────────────────────────

/// Render a single variable declaration line (used inside `globals` block).
pub fn render_var_stmt(src: &str, var: &VarStmt) -> String {
    render_var_stmt_with_renames(src, var, &HashMap::new())
}

pub(crate) fn render_var_stmt_with_renames(src: &str, var: &VarStmt, renames: &RenameMap) -> String {
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
            let name = renamed_opt_name(src, d.name.as_ref(), renames);
            match &d.value {
                Some(v) => format!("{name} = {}", render_expr_with_renames(src, v, renames)),
                None => name,
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
    render_var_stmt_as_local_with_renames(src, var, &HashMap::new())
}

fn render_var_stmt_as_local_with_renames(src: &str, var: &VarStmt, renames: &RenameMap) -> String {
    let type_name = var
        .type_id
        .as_ref()
        .map(|id| id_str(src, id))
        .unwrap_or("integer");

    let mut lines = Vec::new();
    for d in &var.decls {
        let name = renamed_opt_name(src, d.name.as_ref(), renames);
        let mut line = format!("local {} ", type_name);
        if var.is_array {
            line.push_str("array ");
        }
        line.push_str(&name);
        if !var.is_array {
            if let Some(v) = &d.value {
                line.push_str(" = ");
                line.push_str(&render_expr_with_renames(src, v, renames));
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
    reserved_return_names: HashSet<String>,
    variable_renames: RenameMap,
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
            reserved_return_names: HashSet::new(),
            variable_renames: HashMap::new(),
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

    pub(crate) fn for_function_with_reserved(
        function_name: &str,
        return_type: &str,
        reserved_names: &HashSet<String>,
    ) -> Self {
        let mut state = Self::for_function(function_name, return_type);
        for name in reserved_names {
            state.reserved_return_names.insert(name.clone());
        }
        // Also pre-populate declared_names with reserved names so that
        // ensure_return_temp_local will avoid collision with all reserved identifiers
        // including previously-generated return temps.
        for name in reserved_names {
            state.declared_names.insert(name.clone());
        }
        state
    }

    fn declare(&mut self, name: &str) -> bool {
        self.declared_names.insert(name.to_string())
    }

    pub(crate) fn set_variable_renames(&mut self, renames: RenameMap) {
        self.variable_renames = renames;
    }

    pub(crate) fn extend_variable_renames(&mut self, renames: &RenameMap) {
        self.variable_renames.extend(renames.clone());
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
        while self.declared_names.contains(&candidate)
            || self.reserved_return_names.contains(&candidate)
        {
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
    collect_ids_in_expr_with_renames(src, expr, &HashMap::new(), out)
}

fn collect_ids_in_expr_with_renames(
    src: &str,
    expr: &Expr,
    renames: &RenameMap,
    out: &mut HashSet<String>,
) {
    match expr {
        Expr::Id(id) => {
            out.insert(renamed_name(src, id, renames));
        }
        Expr::Call(call) => {
            for arg in &call.args {
                collect_ids_in_expr_with_renames(src, arg, renames, out);
            }
        }
        Expr::FuncRef(_) | Expr::Literal(_) => {}
        Expr::Binary { left, right, .. } => {
            collect_ids_in_expr_with_renames(src, left, renames, out);
            collect_ids_in_expr_with_renames(src, right, renames, out);
        }
        Expr::Unary { operand, .. } | Expr::Parens { inner: operand, .. } => {
            collect_ids_in_expr_with_renames(src, operand, renames, out);
        }
        Expr::Index { array, index, .. } => {
            collect_ids_in_expr_with_renames(src, array, renames, out);
            collect_ids_in_expr_with_renames(src, index, renames, out);
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
    collect_ids_in_expr_with_renames(src, expr, &state.variable_renames, &mut ids);
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
    extract_null_guard_with_renames(src, expr, &HashMap::new())
}

fn extract_null_guard_with_renames(src: &str, expr: &Expr, renames: &RenameMap) -> Option<NullGuard> {
    match expr {
        Expr::Binary { left, right, .. } => {
            let op = operator_between(src, left.cst_node().end_byte(), right.cst_node().start_byte());
            let (var_name, is_neq) = match op {
                "!=" => {
                    if is_null_expr(src, right) {
                        if let Expr::Id(id) = left.as_ref() {
                            (renamed_name(src, id, renames), true)
                        } else {
                            return None;
                        }
                    } else if is_null_expr(src, left) {
                        if let Expr::Id(id) = right.as_ref() {
                            (renamed_name(src, id, renames), true)
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
                            (renamed_name(src, id, renames), false)
                        } else {
                            return None;
                        }
                    } else if is_null_expr(src, left) {
                        if let Expr::Id(id) = right.as_ref() {
                            (renamed_name(src, id, renames), false)
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
        Expr::Parens { inner, .. } => extract_null_guard_with_renames(src, inner, renames),
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
    let name = renamed_opt_name(src, local.name.as_ref(), &state.variable_renames);
    if name.is_empty() {
        return;
    }

    if state.declare(&name) {
        let init = if local.is_array {
            None
        } else {
            Some(default_value_for_type(type_name))
        };
        state
            .hoisted_local_lines
            .push(local_decl_line(type_name, local.is_array, &name, init));
        register_handle_local(state, null_map, type_name, local.is_array, &name);
    }

    if !local.is_array {
        if let Some(v) = &local.value {
            out.push(format!(
                "set {} = {}",
                name,
                render_expr_with_renames(src, v, &state.variable_renames)
            ));
            update_handle_set_state(src, null_map, &name, Some(v));
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
        let name = renamed_opt_name(src, decl.name.as_ref(), &state.variable_renames);
        if name.is_empty() {
            continue;
        }

        if state.declare(&name) {
            let init = if var.is_array {
                None
            } else {
                Some(default_value_for_type(type_name))
            };
            state
                .hoisted_local_lines
                .push(local_decl_line(type_name, var.is_array, &name, init));
            register_handle_local(state, null_map, type_name, var.is_array, &name);
        }

        if !var.is_array {
            if let Some(v) = &decl.value {
                out.push(format!(
                    "set {} = {}",
                    name,
                    render_expr_with_renames(src, v, &state.variable_renames)
                ));
                update_handle_set_state(src, null_map, &name, Some(v));
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
        lines.push(render_return_stmt_with_renames(src, ret, &state.variable_renames));
        return lines;
    }

    let needs_temp = ret
        .value
        .as_ref()
        .map(|expr| expr_references_live_handle_local(src, expr, state, &state.null_map))
        .unwrap_or(false);

    if needs_temp {
        if let (Some(expr), Some(temp_name)) = (ret.value.as_ref(), state.ensure_return_temp_local()) {
            lines.push(format!(
                "set {} = {}",
                temp_name,
                render_expr_with_renames(src, expr, &state.variable_renames)
            ));
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
    lines.push(render_return_stmt_with_renames(src, ret, &state.variable_renames));
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
                let rendered = render_local_decl_with_renames(src, local, &state.variable_renames);
                for line in rendered.lines() {
                    out.push(line.to_string());
                }
                if let Some(name) = &local.name {
                    let name = renamed_name(src, name, &state.variable_renames);
                    state.declare(&name);
                    let type_name = local
                        .type_id
                        .as_ref()
                        .map(|id| id_str(src, id))
                        .unwrap_or("integer");
                    register_handle_local(state, null_map, type_name, local.is_array, &name);
                    if !local.is_array {
                        null_map.insert(name, null_state_from_value(src, local.value.as_ref()));
                    }
                }
            }
            Statement::VarStmt(var) if allow_early_locals && !seen_executable => {
                let rendered = render_var_stmt_as_local_with_renames(src, var, &state.variable_renames);
                for line in rendered.lines() {
                    out.push(line.to_string());
                }
                for decl in &var.decls {
                    if let Some(name) = &decl.name {
                        let name = renamed_name(src, name, &state.variable_renames);
                        state.declare(&name);
                        let type_name = var
                            .type_id
                            .as_ref()
                            .map(|id| id_str(src, id))
                            .unwrap_or("integer");
                        register_handle_local(state, null_map, type_name, var.is_array, &name);
                        if !var.is_array {
                            null_map.insert(name, null_state_from_value(src, decl.value.as_ref()));
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
                    .map(|c| render_expr_with_renames(src, c, &state.variable_renames))
                    .unwrap_or_default();
                let mut lines = vec![format!("if {cond} then")];

                let mut all_return = true;
                let mut merged: Option<NullMap> = None;
                let mut returning_guards = Vec::<NullGuard>::new();
                let has_else = s.branches.iter().any(|b| b.condition.is_none());

                let mut first_map = null_map.clone();
                let first_guard = s
                    .condition
                    .as_ref()
                    .and_then(|c| extract_null_guard_with_renames(src, c, &state.variable_renames));
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
                        Some(c) => lines.push(format!(
                            "elseif {} then",
                            render_expr_with_renames(src, c, &state.variable_renames)
                        )),
                        None => lines.push("else".to_string()),
                    }

                    let mut branch_map = null_map.clone();
                    let guard = branch
                        .condition
                        .as_ref()
                        .and_then(|c| extract_null_guard_with_renames(src, c, &state.variable_renames));
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
                out.push(render_set_stmt_with_renames(src, s, &state.variable_renames));
                if s.index.is_none() {
                    if let Some(var) = &s.variable {
                        let var_name = renamed_name(src, var, &state.variable_renames);
                        update_handle_set_state(src, null_map, &var_name, s.value.as_ref());
                    }
                }
            }
            Statement::Call(s) => {
                seen_executable = true;
                out.push(render_call_stmt_with_renames(src, s, &state.variable_renames));
            }
            Statement::Return(s) => {
                state.null_map = null_map.clone();
                out.extend(render_return_with_leak_fix(src, s, state));
                *null_map = state.null_map.clone();
                return (out, true);
            }
            Statement::Exitwhen(s) => {
                seen_executable = true;
                out.push(render_exitwhen_stmt_with_renames(src, s, &state.variable_renames));
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
    render_globals_vars_with_renames(src, globals, &HashMap::new())
}

pub(crate) fn render_globals_vars_with_renames(
    src: &str,
    globals: &GlobalsBlock,
    renames: &RenameMap,
) -> Vec<String> {
    globals
        .vars
        .iter()
        .map(|v| render_var_stmt_with_renames(src, v, renames))
        .collect()
}

/// Render a complete `function ... endfunction` block.
///
/// Keeps backward-compatible behavior by rendering with an empty set of
/// externally reserved names.
pub fn render_function(src: &str, func: &FunctionDecl) -> (String, Vec<String>) {
    render_function_with_reserved_and_renames(
        src,
        func,
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
    )
}

/// Same as [`render_function`], but also reserves external identifiers that
/// the generated return-temp names must not collide with.
pub fn render_function_with_reserved(
    src: &str,
    func: &FunctionDecl,
    reserved_names: &HashSet<String>,
) -> (String, Vec<String>) {
    render_function_with_reserved_and_renames(
        src,
        func,
        reserved_names,
        reserved_names,
        &HashMap::new(),
    )
}

pub(crate) fn render_function_with_reserved_and_renames(
    src: &str,
    func: &FunctionDecl,
    reserved_names: &HashSet<String>,
    function_names: &HashSet<String>,
    external_renames: &RenameMap,
) -> (String, Vec<String>) {
    let prefix = if func.is_constant { "constant function " } else { "function " };
    let name = func.name.as_ref().map(|id| id_str(src, id)).unwrap_or("");
    let params_renames = build_function_rename_map(src, func, function_names);
    let all_renames = merge_rename_maps(external_renames, &params_renames);
    let params = render_params_with_renames(src, &func.params, &all_renames);
    let ret = func
        .return_type
        .as_ref()
        .map(|id| id_str(src, id))
        .unwrap_or("nothing");

    let mut out = format!("{prefix}{name} takes {params} returns {ret}\n");

    let mut state = HoistRenderState::for_function_with_reserved(name, ret, reserved_names);
    state.set_variable_renames(all_renames);
    // Pre-populate declared names with parameter names so the return-temp
    // generator cannot collide with any parameter.
    for p in &func.params {
        if let Some(id) = &p.name {
            let rendered = renamed_name(src, id, &state.variable_renames);
            state.declare(&rendered);
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

