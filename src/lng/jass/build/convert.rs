//! AST → IR conversion.
//!
//! Converts tree-sitter AST nodes (`Expr`, `Statement`, `FunctionDecl`) into
//! the owned IR types (`IRExpr`, `IRStmt`, `IRFunc`), and collects the complete
//! [`BuildIR`] from all source files in the import tree.

use crate::lng::jass::ast::{
    build_ast, rewrite_imports, Expr, FunctionDecl, Id, Statement,
};
use crate::util::file_store::{is_uri_frozen, FILE_STORE};
use std::collections::{HashMap, HashSet};
use url::Url;

use super::inline::detect_inline_candidate;
use super::ir::*;
use super::io::read_file_source;

// ─── Text extraction helpers ─────────────────────────────────────────────────

/// Collapse a CST node's text to a single line (all whitespace → single space).
pub(super) fn flatten(src: &str, node: &tree_sitter::Node) -> String {
    let text = &src[node.start_byte()..node.end_byte()];
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Read raw identifier text.
pub(super) fn id_text(src: &str, id: &Id) -> String {
    src[id.node.start_byte()..id.node.end_byte()].to_string()
}

/// Extract the operator text from a binary expression by looking at the
/// source between the left and right operand CST spans.
pub(super) fn binary_op_text(src: &str, left: &Expr, right: &Expr) -> String {
    let left_end = left.cst_node().end_byte();
    let right_start = right.cst_node().start_byte();
    if left_end < right_start {
        src[left_end..right_start].trim().to_string()
    } else {
        String::new()
    }
}

// ─── AST → IR conversion ────────────────────────────────────────────────────

pub(super) fn convert_expr(src: &str, expr: &Expr) -> IRExpr {
    match expr {
        Expr::Id(id) => IRExpr::Id(id_text(src, id)),
        Expr::Literal(node) => IRExpr::Literal(flatten(src, node)),
        Expr::Call(fc) => {
            let name = fc.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
            let args = fc.args.iter().map(|a| convert_expr(src, a)).collect();
            IRExpr::Call { name, args }
        }
        Expr::FuncRef(id) => IRExpr::FuncRef(id_text(src, id)),
        Expr::Binary { left, right, .. } => {
            let op = binary_op_text(src, left, right);
            IRExpr::Binary {
                left: Box::new(convert_expr(src, left)),
                op,
                right: Box::new(convert_expr(src, right)),
            }
        }
        Expr::Unary { node, operand } => {
            let op_end = operand.cst_node().start_byte();
            let op = src[node.start_byte()..op_end].trim().to_string();
            IRExpr::Unary {
                op,
                operand: Box::new(convert_expr(src, operand)),
            }
        }
        Expr::Parens { inner, .. } => {
            IRExpr::Parens(Box::new(convert_expr(src, inner)))
        }
        Expr::Index { array, index, .. } => {
            IRExpr::Index {
                array: Box::new(convert_expr(src, array)),
                index: Box::new(convert_expr(src, index)),
            }
        }
    }
}

pub(super) fn convert_stmt(src: &str, stmt: &Statement) -> Option<IRStmt> {
    match stmt {
        Statement::Local(l) => {
            let type_name = l.type_id.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "integer".into());
            let name = l.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
            let value = l.value.as_ref().map(|e| convert_expr(src, e));
            Some(IRStmt::Local { type_name, is_array: l.is_array, name, value })
        }
        Statement::Set(s) => {
            let var = s.variable.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
            let index = s.index.as_ref().map(|e| convert_expr(src, e));
            let value = s.value.as_ref().map(|e| convert_expr(src, e)).unwrap_or(IRExpr::int(0));
            Some(IRStmt::Set { var, index, value })
        }
        Statement::Call(c) => {
            if let Some(fc) = &c.func {
                let name = fc.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
                let args = fc.args.iter().map(|a| convert_expr(src, a)).collect();
                Some(IRStmt::Call { name, args })
            } else {
                None
            }
        }
        Statement::Return(r) => {
            Some(IRStmt::Return(r.value.as_ref().map(|e| convert_expr(src, e))))
        }
        Statement::Exitwhen(e) => {
            Some(IRStmt::Exitwhen(
                e.condition.as_ref().map(|c| convert_expr(src, c)).unwrap_or(IRExpr::bool_val(true))
            ))
        }
        Statement::If(i) => {
            let condition = i.condition.as_ref()
                .map(|c| convert_expr(src, c))
                .unwrap_or(IRExpr::bool_val(true));
            let body = convert_body(src, &i.body);
            let branches = i.branches.iter().map(|b| IRBranch {
                condition: b.condition.as_ref().map(|c| convert_expr(src, c)),
                body: convert_body(src, &b.body),
            }).collect();
            Some(IRStmt::If { condition, body, branches })
        }
        Statement::Loop(l) => {
            Some(IRStmt::Loop(convert_body(src, &l.body)))
        }
        Statement::VarStmt(v) => {
            let type_name = v.type_id.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "integer".into());
            let decls = v.decls.iter().map(|d| IRVarInit {
                name: d.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default(),
                value: d.value.as_ref().map(|e| convert_expr(src, e)),
            }).collect();
            Some(IRStmt::VarDecl { is_constant: v.is_constant, is_array: v.is_array, type_name, decls })
        }
        _ => None,
    }
}

pub(super) fn convert_body(src: &str, stmts: &[Statement]) -> Vec<IRStmt> {
    stmts.iter().filter_map(|s| convert_stmt(src, s)).collect()
}

pub(super) fn convert_function(
    src: &str,
    f: &FunctionDecl,
    callees: HashSet<String>,
) -> IRFunc {
    let name = f.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
    let params: Vec<(String, String)> = f.params.iter().map(|p| {
        let t = p.type_id.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "integer".into());
        let n = p.name.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "_".into());
        (t, n)
    }).collect();
    let return_type = f.return_type.as_ref()
        .map(|id| id_text(src, id))
        .unwrap_or_else(|| "nothing".into());
    let body = convert_body(src, &f.body);

    // Detect inline candidate: takes nothing + single `return expr`.
    let inline_expr = if f.params.is_empty() {
        detect_inline_candidate(src, &f.body, false)
    } else {
        None
    };

    IRFunc { name, params, return_type, body, callees, inline_expr }
}

// ─── IR collection from source files ─────────────────────────────────────────

/// Collect the IR from all **non-frozen** source files.
///
/// Also collects native names from ALL files (including frozen) so the
/// AS build can later prefix them with `Jass::`.
///
/// Frozen-file functions and globals are **not** included here — call
/// [`resolve_frozen_deps`] after augmentation to add them.
pub(super) fn collect_ir(_trigger_uri: &Url, file_order: &[Url]) -> BuildIR {
    let mut globals = Vec::<IRStmt>::new();
    let mut functions: HashMap<String, IRFunc> = HashMap::new();
    let mut bare_stmts = Vec::<IRStmt>::new();
    let mut native_names = HashSet::<String>::new();

    // Collect native names from FILE_STORE for all files in the import tree.
    for file_uri in file_order {
        if let Some(fs) = FILE_STORE.get(file_uri) {
            for n in &fs.file_symbols.natives {
                native_names.insert(n.name.clone());
            }
        }
    }

    for file_uri in file_order {
        if is_uri_frozen(file_uri) { continue; }

        let src = match read_file_source(file_uri) {
            Some(s) => s,
            None => continue,
        };

        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&tree_sitter_jass::language().into()).is_err() { continue; }
        let tree = match parser.parse(&src, None) {
            Some(t) => t,
            None => continue,
        };

        let mut ast = build_ast(tree.root_node());
        let src_bytes = src.as_bytes().to_vec();
        rewrite_imports(&mut ast, &src_bytes);

        for item in &ast.items {
            match item {
                Statement::Type(_) | Statement::Native(_) => {}
                Statement::Globals(g) => {
                    for v in &g.vars {
                        if let Some(s) = convert_stmt(&src, &Statement::VarStmt(v.clone())) {
                            globals.push(s);
                        }
                    }
                }
                Statement::Function(f) => {
                    let fname = f.name.as_ref().map(|id| id_text(&src, id)).unwrap_or_default();
                    if !fname.is_empty() {
                        let callees: HashSet<String> = FILE_STORE
                            .get(file_uri)
                            .map(|fs| {
                                fs.file_symbols.functions.iter()
                                    .find(|ff| ff.name == fname)
                                    .map(|ff| ff.callees.clone())
                                    .unwrap_or_default()
                            })
                            .unwrap_or_default();
                        functions.insert(fname.clone(), convert_function(&src, f, callees));
                    }
                }
                Statement::VarStmt(v) => {
                    if let Some(s) = convert_stmt(&src, &Statement::VarStmt(v.clone())) {
                        globals.push(s);
                    }
                }
                Statement::Set(_) | Statement::Call(_) | Statement::If(_) | Statement::Loop(_) => {
                    if let Some(s) = convert_stmt(&src, item) {
                        bare_stmts.push(s);
                    }
                }
                _ => {}
            }
        }
    }

    BuildIR { globals, functions, bare_stmts, native_names }
}

// ─── Frozen-file dependency resolution (AS builds) ───────────────────────────

/// Add reachable frozen-file functions and their referenced globals to the IR.
///
/// **Must be called AFTER** `augment_main` / `augment_config` and the
/// bare-stmts merge into `main`, so that the call graph includes all
/// generated calls (e.g. `InitBlizzard`, `SetPlayerAllianceStateAllyBJ`).
///
/// The algorithm:
/// 1. Walk every IR function body to discover call targets (not the
///    `callees` field — it doesn't reflect augmented calls).
/// 2. BFS through frozen FILE_STORE callees → set of needed frozen functions.
/// 3. Parse frozen files, extract only needed functions + candidate globals.
/// 4. Walk all function bodies again to find referenced globals; keep only those.
pub(super) fn resolve_frozen_deps(ir: &mut BuildIR, file_order: &[Url]) {
    // 1. Build a map: frozen_function_name → its callees (from FILE_STORE).
    let mut frozen_func_callees: HashMap<String, HashSet<String>> = HashMap::new();
    for file_uri in file_order {
        if !is_uri_frozen(file_uri) { continue; }
        if let Some(fs) = FILE_STORE.get(file_uri) {
            for f in &fs.file_symbols.functions {
                frozen_func_callees.insert(f.name.clone(), f.callees.clone());
            }
        }
    }
    if frozen_func_callees.is_empty() { return; }

    // 2. Seed: walk ALL current IR function bodies to find every call target.
    //    This picks up augmented calls like InitBlizzard, SetPlayerAllianceStateBJ, etc.
    let mut worklist: Vec<String> = Vec::new();
    for func in ir.functions.values() {
        collect_call_names_in_stmts(&func.body, &mut worklist);
    }

    // 3. BFS: transitively expand through frozen functions.
    let mut needed_funcs: HashSet<String> = HashSet::new();
    while let Some(name) = worklist.pop() {
        if !frozen_func_callees.contains_key(&name) { continue; }
        if !needed_funcs.insert(name.clone()) { continue; }
        if let Some(callees) = frozen_func_callees.get(&name) {
            worklist.extend(callees.iter().cloned());
        }
    }
    if needed_funcs.is_empty() { return; }

    // 4. Parse frozen files — extract only needed functions + all globals.
    let mut frozen_globals: Vec<IRStmt> = Vec::new();

    for file_uri in file_order {
        if !is_uri_frozen(file_uri) { continue; }

        let src = match read_file_source(file_uri) {
            Some(s) => s,
            None => continue,
        };

        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&tree_sitter_jass::language().into()).is_err() { continue; }
        let tree = match parser.parse(&src, None) {
            Some(t) => t,
            None => continue,
        };

        let mut ast = build_ast(tree.root_node());
        let src_bytes = src.as_bytes().to_vec();
        rewrite_imports(&mut ast, &src_bytes);

        for item in &ast.items {
            match item {
                Statement::Type(_) | Statement::Native(_) => {}
                Statement::Globals(g) => {
                    for v in &g.vars {
                        if let Some(s) = convert_stmt(&src, &Statement::VarStmt(v.clone())) {
                            frozen_globals.push(s);
                        }
                    }
                }
                Statement::Function(f) => {
                    let fname = f.name.as_ref().map(|id| id_text(&src, id)).unwrap_or_default();
                    if !fname.is_empty() && needed_funcs.contains(&fname) && !ir.functions.contains_key(&fname) {
                        let callees: HashSet<String> = FILE_STORE
                            .get(file_uri)
                            .map(|fs| {
                                fs.file_symbols.functions.iter()
                                    .find(|ff| ff.name == fname)
                                    .map(|ff| ff.callees.clone())
                                    .unwrap_or_default()
                            })
                            .unwrap_or_default();
                        ir.functions.insert(fname.clone(), convert_function(&src, f, callees));
                    }
                }
                Statement::VarStmt(v) => {
                    if let Some(s) = convert_stmt(&src, &Statement::VarStmt(v.clone())) {
                        frozen_globals.push(s);
                    }
                }
                _ => {} // skip bare statements from frozen files
            }
        }
    }

    // 5. Determine which frozen globals are actually referenced.
    let frozen_global_names: HashSet<String> = frozen_globals.iter()
        .filter_map(|s| match s {
            IRStmt::VarDecl { decls, .. } => Some(decls.iter().map(|d| d.name.clone())),
            _ => None,
        })
        .flatten()
        .collect();

    if !frozen_global_names.is_empty() {
        // Walk ALL function bodies (user + newly-added frozen) to find
        // referenced identifiers.
        let mut referenced = HashSet::new();
        for func in ir.functions.values() {
            referenced.extend(collect_referenced_globals(func));
        }

        // Prepend only the frozen globals that are actually used.
        // (Frozen files are logically "earlier" — their globals go first.)
        let user_globals = std::mem::take(&mut ir.globals);
        for stmt in frozen_globals {
            if let IRStmt::VarDecl { ref decls, .. } = stmt {
                if decls.iter().any(|d| frozen_global_names.contains(&d.name)
                    && referenced.contains(&d.name))
                {
                    ir.globals.push(stmt);
                }
            }
        }
        ir.globals.extend(user_globals);
    }
}

// ─── IR call-name extraction (walks actual bodies, not `callees` field) ──────

/// Walk statements and collect all function names that appear in `Call` or
/// `FuncRef` nodes.
fn collect_call_names_in_stmts(stmts: &[IRStmt], out: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            IRStmt::Call { name, args } => {
                out.push(name.clone());
                for a in args { collect_call_names_in_expr(a, out); }
            }
            IRStmt::Set { index, value, .. } => {
                if let Some(idx) = index { collect_call_names_in_expr(idx, out); }
                collect_call_names_in_expr(value, out);
            }
            IRStmt::Local { value, .. } => {
                if let Some(v) = value { collect_call_names_in_expr(v, out); }
            }
            IRStmt::Return(v) => {
                if let Some(v) = v { collect_call_names_in_expr(v, out); }
            }
            IRStmt::Exitwhen(cond) => {
                collect_call_names_in_expr(cond, out);
            }
            IRStmt::If { condition, body, branches } => {
                collect_call_names_in_expr(condition, out);
                collect_call_names_in_stmts(body, out);
                for b in branches {
                    if let Some(ref c) = b.condition { collect_call_names_in_expr(c, out); }
                    collect_call_names_in_stmts(&b.body, out);
                }
            }
            IRStmt::Loop(body) => {
                collect_call_names_in_stmts(body, out);
            }
            IRStmt::VarDecl { decls, .. } => {
                for d in decls {
                    if let Some(ref v) = d.value { collect_call_names_in_expr(v, out); }
                }
            }
        }
    }
}

fn collect_call_names_in_expr(expr: &IRExpr, out: &mut Vec<String>) {
    match expr {
        IRExpr::Call { name, args } => {
            out.push(name.clone());
            for a in args { collect_call_names_in_expr(a, out); }
        }
        IRExpr::FuncRef(name) => {
            out.push(name.clone());
        }
        IRExpr::Binary { left, right, .. } => {
            collect_call_names_in_expr(left, out);
            collect_call_names_in_expr(right, out);
        }
        IRExpr::Unary { operand, .. } => {
            collect_call_names_in_expr(operand, out);
        }
        IRExpr::Parens(inner) => {
            collect_call_names_in_expr(inner, out);
        }
        IRExpr::Index { array, index } => {
            collect_call_names_in_expr(array, out);
            collect_call_names_in_expr(index, out);
        }
        IRExpr::Id(_) | IRExpr::Literal(_) => {}
    }
}

// ─── Reachability helpers: collect identifier references from IR ──────────────

/// Collect non-local identifiers referenced (read or written) in a function.
fn collect_referenced_globals(func: &IRFunc) -> HashSet<String> {
    let mut locals = HashSet::new();
    for (_, name) in &func.params {
        locals.insert(name.clone());
    }
    collect_local_names(&func.body, &mut locals);

    let mut ids = HashSet::new();
    collect_ids_in_stmts(&func.body, &locals, &mut ids);
    ids
}

/// Recursively collect all local/parameter names declared in statements.
fn collect_local_names(stmts: &[IRStmt], locals: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            IRStmt::Local { name, .. } => { locals.insert(name.clone()); }
            IRStmt::VarDecl { decls, .. } => {
                for d in decls { locals.insert(d.name.clone()); }
            }
            IRStmt::If { body, branches, .. } => {
                collect_local_names(body, locals);
                for b in branches { collect_local_names(&b.body, locals); }
            }
            IRStmt::Loop(body) => { collect_local_names(body, locals); }
            _ => {}
        }
    }
}

/// Walk statements and collect all identifier references that are not in `locals`.
fn collect_ids_in_stmts(stmts: &[IRStmt], locals: &HashSet<String>, out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            IRStmt::Set { var, index, value } => {
                if !locals.contains(var) { out.insert(var.clone()); }
                if let Some(idx) = index { collect_ids_in_expr(idx, locals, out); }
                collect_ids_in_expr(value, locals, out);
            }
            IRStmt::Local { value, .. } => {
                if let Some(v) = value { collect_ids_in_expr(v, locals, out); }
            }
            IRStmt::Call { args, .. } => {
                for a in args { collect_ids_in_expr(a, locals, out); }
            }
            IRStmt::Return(v) => {
                if let Some(v) = v { collect_ids_in_expr(v, locals, out); }
            }
            IRStmt::Exitwhen(cond) => {
                collect_ids_in_expr(cond, locals, out);
            }
            IRStmt::If { condition, body, branches } => {
                collect_ids_in_expr(condition, locals, out);
                collect_ids_in_stmts(body, locals, out);
                for b in branches {
                    if let Some(ref c) = b.condition { collect_ids_in_expr(c, locals, out); }
                    collect_ids_in_stmts(&b.body, locals, out);
                }
            }
            IRStmt::Loop(body) => {
                collect_ids_in_stmts(body, locals, out);
            }
            IRStmt::VarDecl { decls, .. } => {
                for d in decls {
                    if let Some(ref v) = d.value { collect_ids_in_expr(v, locals, out); }
                }
            }
        }
    }
}

/// Walk an expression and collect all identifier references that are not in `locals`.
fn collect_ids_in_expr(expr: &IRExpr, locals: &HashSet<String>, out: &mut HashSet<String>) {
    match expr {
        IRExpr::Id(name) => {
            if !locals.contains(name) { out.insert(name.clone()); }
        }
        IRExpr::Call { args, .. } => {
            for a in args { collect_ids_in_expr(a, locals, out); }
        }
        IRExpr::Binary { left, right, .. } => {
            collect_ids_in_expr(left, locals, out);
            collect_ids_in_expr(right, locals, out);
        }
        IRExpr::Unary { operand, .. } => {
            collect_ids_in_expr(operand, locals, out);
        }
        IRExpr::Parens(inner) => {
            collect_ids_in_expr(inner, locals, out);
        }
        IRExpr::Index { array, index } => {
            collect_ids_in_expr(array, locals, out);
            collect_ids_in_expr(index, locals, out);
        }
        IRExpr::Literal(_) | IRExpr::FuncRef(_) => {}
    }
}

