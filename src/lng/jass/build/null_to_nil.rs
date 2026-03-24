//! IR pass: rewrite `null` → `nil` for handle-typed contexts (AS build).
//!
//! In AngelScript the null-handle literal is spelled `nil`, not `null`.
//! This pass walks the owned IR and replaces every `IRExpr::Literal("null")`
//! with `IRExpr::Literal("nil")` when the *expected type* at that position
//! is a handle-derived type.
//!
//! Type information comes from the IR itself:
//! - `IRStmt::Local` / `IRStmt::VarDecl` carry `type_name`.
//! - `IRFunc::params` and `IRFunc::return_type`.
//! - Function signatures from `FILE_STORE` (natives, frozen-file functions).
//!
//! For `==` / `!=` comparisons the type of the *other* operand is inferred
//! so that `u == null` becomes `u == nil` when `u` is a handle variable.

use super::ir::*;
use crate::util::file_store::FILE_STORE;
use std::collections::HashMap;

// ─── Handle-type predicate ───────────────────────────────────────────────────

/// A type is handle-derived if it is *not* one of the built-in primitives.
fn is_handle_type(type_name: &str) -> bool {
    !matches!(
        type_name,
        "integer" | "real" | "boolean" | "string" | "code" | "nothing" | "null" | "unknown"
    )
}

// ─── Shared (global) type context ────────────────────────────────────────────

/// Immutable context built once from the full [`BuildIR`] + `FILE_STORE`.
struct GlobalCtx {
    /// `func_name → [param_type, …]`
    func_params: HashMap<String, Vec<String>>,
    /// `func_name → return_type`
    func_returns: HashMap<String, String>,
    /// `var_name → type_name` (global variables).
    global_vars: HashMap<String, String>,
}

impl GlobalCtx {
    /// Build from the complete IR (after frozen-dep resolution) + FILE_STORE.
    fn from_ir(ir: &BuildIR) -> Self {
        let mut func_params = HashMap::new();
        let mut func_returns = HashMap::new();
        let mut global_vars = HashMap::new();

        // IR functions.
        for (name, func) in &ir.functions {
            let types: Vec<String> = func.params.iter().map(|(t, _)| t.clone()).collect();
            func_params.insert(name.clone(), types);
            func_returns.insert(name.clone(), func.return_type.clone());
        }

        // FILE_STORE — natives and functions not yet in the IR.
        for entry in FILE_STORE.iter() {
            let symbols = &entry.value().file_symbols;
            for f in &symbols.functions {
                func_params.entry(f.name.clone()).or_insert_with(|| {
                    f.params.iter().map(|p| p.type_name.clone()).collect()
                });
                func_returns.entry(f.name.clone()).or_insert_with(|| {
                    f.return_type.clone().unwrap_or_else(|| "nothing".into())
                });
            }
            for n in &symbols.natives {
                func_params.entry(n.name.clone()).or_insert_with(|| {
                    n.params.iter().map(|p| p.type_name.clone()).collect()
                });
                func_returns.entry(n.name.clone()).or_insert_with(|| {
                    n.return_type.clone().unwrap_or_else(|| "nothing".into())
                });
            }
        }

        // Global variable types.
        for stmt in &ir.globals {
            if let IRStmt::VarDecl { type_name, decls, .. } = stmt {
                for d in decls {
                    global_vars.insert(d.name.clone(), type_name.clone());
                }
            }
        }

        GlobalCtx { func_params, func_returns, global_vars }
    }

    /// Minimal context without FILE_STORE (used in the test pipeline).
    #[cfg(test)]
    fn empty() -> Self {
        GlobalCtx {
            func_params: HashMap::new(),
            func_returns: HashMap::new(),
            global_vars: HashMap::new(),
        }
    }
}

// ─── Per-function type context ───────────────────────────────────────────────

/// Per-function context: borrows the shared global part and owns the local
/// variable type map (locals + function parameters).
struct FuncCtx<'a> {
    global: &'a GlobalCtx,
    /// `var_name → type_name` — locals + parameters.
    local_vars: HashMap<String, String>,
}

impl<'a> FuncCtx<'a> {
    fn var_type(&self, name: &str) -> Option<&str> {
        self.local_vars
            .get(name)
            .map(|s| s.as_str())
            .or_else(|| self.global.global_vars.get(name).map(|s| s.as_str()))
    }

    fn func_param_types(&self, name: &str) -> Option<&[String]> {
        self.global.func_params.get(name).map(|v| v.as_slice())
    }

    fn func_return_type(&self, name: &str) -> Option<&str> {
        self.global.func_returns.get(name).map(|s| s.as_str())
    }
}

// ─── Local type collection ───────────────────────────────────────────────────

/// Recursively collect all variable types declared in a statement list.
fn collect_local_types(stmts: &[IRStmt], map: &mut HashMap<String, String>) {
    for stmt in stmts {
        match stmt {
            IRStmt::Local { type_name, name, .. } => {
                map.insert(name.clone(), type_name.clone());
            }
            IRStmt::VarDecl { type_name, decls, .. } => {
                for d in decls {
                    map.insert(d.name.clone(), type_name.clone());
                }
            }
            IRStmt::If { body, branches, .. } => {
                collect_local_types(body, map);
                for b in branches {
                    collect_local_types(&b.body, map);
                }
            }
            IRStmt::Loop(body) => {
                collect_local_types(body, map);
            }
            _ => {}
        }
    }
}

// ─── Simple expression-type inference ────────────────────────────────────────

/// Best-effort type inference for an expression (used for `==` / `!=` peers).
fn infer_expr_type(expr: &IRExpr, ctx: &FuncCtx) -> Option<String> {
    match expr {
        IRExpr::Id(name) => {
            // `null`, `true`, `false` are parsed as identifiers by tree-sitter-jass.
            match name.as_str() {
                "null" | "nil" => Some("null".to_string()),
                "true" | "false" => Some("boolean".to_string()),
                _ => ctx.var_type(name).map(|s| s.to_string()),
            }
        }
        IRExpr::Call { name, .. } => ctx.func_return_type(name).map(|s| s.to_string()),
        IRExpr::Index { array, .. } => {
            if let IRExpr::Id(name) = array.as_ref() {
                ctx.var_type(name).map(|s| s.to_string())
            } else {
                None
            }
        }
        IRExpr::Parens(inner) => infer_expr_type(inner, ctx),
        IRExpr::FuncRef(_) => Some("code".to_string()),
        IRExpr::Literal(s) => {
            if s == "null" || s == "nil" {
                Some("null".to_string())
            } else if s == "true" || s == "false" {
                Some("boolean".to_string())
            } else if s.starts_with('"') {
                Some("string".to_string())
            } else if s.starts_with('\'') {
                Some("integer".to_string())
            } else if s.contains('.') {
                Some("real".to_string())
            } else {
                Some("integer".to_string())
            }
        }
        _ => None,
    }
}

// ─── Null detection helper ───────────────────────────────────────────────────

/// Check if an expression is a `null` literal (either `Literal("null")` or
/// `Id("null")` — tree-sitter-jass parses `null` as an identifier).
fn is_null_expr(expr: &IRExpr) -> bool {
    matches!(expr, IRExpr::Literal(s) | IRExpr::Id(s) if s == "null")
}

// ─── Expression rewriting ────────────────────────────────────────────────────

/// Rewrite `null` → `nil` inside an expression.
///
/// `expected_type` is the type that the *parent* context expects this
/// expression to produce (e.g. the declared type of a variable being
/// assigned).  When the expression is a bare `null` literal and the
/// expected type is handle-derived, the literal is replaced with `nil`.
///
/// The function also recurses into sub-expressions that carry their own
/// type context (function call arguments, comparison operands).
fn rewrite_expr(expr: &mut IRExpr, expected_type: Option<&str>, ctx: &FuncCtx) {
    match expr {
        // Leaf: `null` literal or identifier — possibly replace.
        IRExpr::Literal(s) | IRExpr::Id(s) if s == "null" => {
            if let Some(ty) = expected_type {
                if is_handle_type(ty) {
                    *s = "nil".into();
                }
            }
        }

        // Function call — args get their types from the callee signature.
        IRExpr::Call { name, args } => {
            let param_types: Option<Vec<String>> =
                ctx.func_param_types(name).map(|v| v.to_vec());
            for (i, arg) in args.iter_mut().enumerate() {
                let expected = param_types
                    .as_ref()
                    .and_then(|types| types.get(i))
                    .map(|s| s.as_str());
                rewrite_expr(arg, expected, ctx);
            }
        }

        // Comparison — infer the type from the non-null side.
        IRExpr::Binary { left, op, right } if op == "==" || op == "!=" => {
            let left_is_null = is_null_expr(left);
            let right_is_null = is_null_expr(right);

            if left_is_null && !right_is_null {
                let peer_type = infer_expr_type(right, ctx);
                rewrite_expr(left, peer_type.as_deref(), ctx);
                rewrite_expr(right, None, ctx);
            } else if right_is_null && !left_is_null {
                let peer_type = infer_expr_type(left, ctx);
                rewrite_expr(right, peer_type.as_deref(), ctx);
                rewrite_expr(left, None, ctx);
            } else {
                rewrite_expr(left, None, ctx);
                rewrite_expr(right, None, ctx);
            }
        }

        // Other binary — just recurse.
        IRExpr::Binary { left, right, .. } => {
            rewrite_expr(left, None, ctx);
            rewrite_expr(right, None, ctx);
        }

        IRExpr::Unary { operand, .. } => {
            rewrite_expr(operand, None, ctx);
        }

        // Parentheses — propagate expected type.
        IRExpr::Parens(inner) => {
            rewrite_expr(inner, expected_type, ctx);
        }

        IRExpr::Index { array, index } => {
            rewrite_expr(array, None, ctx);
            rewrite_expr(index, None, ctx);
        }

        IRExpr::Cast { inner, .. } => {
            rewrite_expr(inner, expected_type, ctx);
        }

        _ => {}
    }
}

// ─── Statement rewriting ─────────────────────────────────────────────────────

fn rewrite_stmt(stmt: &mut IRStmt, return_type: &str, ctx: &FuncCtx) {
    match stmt {
        IRStmt::Local { type_name, value: Some(value), .. } => {
            let expected = Some(type_name.as_str()).filter(|t| is_handle_type(t));
            rewrite_expr(value, expected, ctx);
        }
        IRStmt::Local { .. } => {}

        IRStmt::Set { var, index, value } => {
            let var_type = ctx.var_type(var).map(|s| s.to_string());
            let expected = var_type.as_deref().filter(|t| is_handle_type(t));
            rewrite_expr(value, expected, ctx);
            if let Some(idx) = index {
                rewrite_expr(idx, None, ctx);
            }
        }

        IRStmt::Call { name, args } => {
            let param_types: Option<Vec<String>> =
                ctx.func_param_types(name).map(|v| v.to_vec());
            for (i, arg) in args.iter_mut().enumerate() {
                let expected = param_types
                    .as_ref()
                    .and_then(|types| types.get(i))
                    .map(|s| s.as_str());
                rewrite_expr(arg, expected, ctx);
            }
        }

        IRStmt::Return(Some(value)) => {
            let expected = Some(return_type).filter(|t| is_handle_type(t));
            rewrite_expr(value, expected, ctx);
        }
        IRStmt::Return(None) => {}

        IRStmt::Exitwhen(cond) => {
            rewrite_expr(cond, None, ctx);
        }

        IRStmt::If { condition, body, branches } => {
            rewrite_expr(condition, None, ctx);
            for s in body.iter_mut() {
                rewrite_stmt(s, return_type, ctx);
            }
            for b in branches.iter_mut() {
                if let Some(ref mut cond) = b.condition {
                    rewrite_expr(cond, None, ctx);
                }
                for s in b.body.iter_mut() {
                    rewrite_stmt(s, return_type, ctx);
                }
            }
        }

        IRStmt::Loop(body) => {
            for s in body.iter_mut() {
                rewrite_stmt(s, return_type, ctx);
            }
        }

        IRStmt::VarDecl { type_name, decls, .. } => {
            let expected = Some(type_name.as_str()).filter(|t| is_handle_type(t));
            for d in decls.iter_mut() {
                if let Some(ref mut value) = d.value {
                    rewrite_expr(value, expected, ctx);
                }
            }
        }
    }
}

// ─── Per-function driver ─────────────────────────────────────────────────────

fn rewrite_function(func: &mut IRFunc, global: &GlobalCtx) {
    let mut local_vars = HashMap::new();

    // Function parameters.
    for (type_name, param_name) in &func.params {
        local_vars.insert(param_name.clone(), type_name.clone());
    }
    // All locals (even late-declared ones — JASS allows forward references
    // after hoisting).
    collect_local_types(&func.body, &mut local_vars);

    let ctx = FuncCtx { global, local_vars };
    let return_type = func.return_type.clone();

    for stmt in &mut func.body {
        rewrite_stmt(stmt, &return_type, &ctx);
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Rewrite `null` → `nil` for all handle-typed contexts in a full build IR.
///
/// Call after `resolve_frozen_deps` and before rendering to text.
pub(super) fn rewrite_null_to_nil(ir: &mut BuildIR) {
    let global_ctx = GlobalCtx::from_ir(ir);

    // Globals (no function return-type context).
    {
        let ctx = FuncCtx {
            global: &global_ctx,
            local_vars: HashMap::new(),
        };
        for stmt in &mut ir.globals {
            rewrite_stmt(stmt, "nothing", &ctx);
        }
    }

    // Functions.
    for func in ir.functions.values_mut() {
        rewrite_function(func, &global_ctx);
    }
}

/// Rewrite `null` → `nil` in a single function (test pipeline).
///
/// Registers the function's own signature so that recursive calls and
/// local variable types can be resolved.  External function signatures
/// and global variables are not available.
#[cfg(test)]
pub(super) fn rewrite_func_null_to_nil(func: &mut IRFunc) {
    let mut global_ctx = GlobalCtx::empty();
    // Register the function's own signature.
    let param_types: Vec<String> = func.params.iter().map(|(t, _)| t.clone()).collect();
    global_ctx.func_params.insert(func.name.clone(), param_types);
    global_ctx.func_returns.insert(func.name.clone(), func.return_type.clone());
    rewrite_function(func, &global_ctx);
}

