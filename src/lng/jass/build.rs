//! JASS build — merge the import tree into a single `.j` or `.as` file.
//!
//! Searches the entire connected component of the import tree for
//! `//set build-jass <path>` / `//set build-as <path>` directives.
//!
//! **Frozen files (`//import!`)** are excluded from the build entirely
//! in both JASS and AS modes — they are engine-provided / read-only.
//!
//! **`type` and `native` declarations** are never included in the build
//! output — they are engine-provided and do not belong in the merged file.
//!
//! Output structure (JASS):
//! 1. `globals … endglobals` (merged from all files)
//! 2. Functions in topological order (callee above caller)
//! 3. Bare top-level statements → synthesized `function main takes nothing returns nothing … endfunction`
//!
//! Output structure (AngelScript):
//! 1. Global variable declarations
//! 2. Functions in topological order
//! 3. Bare top-level statements → synthesized `void main() { … }`

use crate::lng::jass::ast::{
    build_ast, rewrite_imports, CallStmt, Expr, ExitwhenStmt, FunctionDecl, Id,
    IfStmt, LocalDecl, ReturnStmt, SetStmt, Statement, VarStmt,
};
use crate::lng::jass::kind::Kind;
use crate::util::file_store::{is_uri_frozen, FILE_STORE};
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::string_hash::{collect_constants, fold_string_hash, fold_string_integer_args};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use url::Url;

/// Build mode — determines how frozen (`//import!`) files are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildMode {
    /// JASS build: frozen files are skipped entirely.
    Jass,
    /// AngelScript build: frozen files contribute only function forward
    /// declarations (as stubs), so the AS compiler knows their signatures.
    As,
}

// ─── Public API ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct BuildResult {
    /// `true` when the file was successfully written.
    pub ok: bool,
    /// Path where the output was written (empty on error).
    pub path: String,
    /// Human-readable message (success or error description).
    pub message: String,
}

/// Execute the JASS build for the file at `uri`.
///
/// The build always starts from an `//entry` file.  The entry file must
/// carry a `//set build-jass <path>` directive (or one of the entry files
/// reachable in the same connected component must have it).
///
/// Files not reachable from the entry point are excluded (tree-shaking).
/// If no `//entry` file with a `build-jass` setting is found, an error
/// is returned — the build requires an explicit entry point.
pub fn build_jass(uri: &Url) -> BuildResult {
    // 1. Find build target — must be in an //entry file when entries exist.
    let (trigger_uri, target) = match find_build_setting(uri, "build-jass") {
        Some(pair) => pair,
        None => return err(crate::util::i18n::build_no_setting_jass()),
    };

    // 2. Resolve output path relative to the file that owns the directive.
    let base_dir = match trigger_uri.to_file_path() {
        Ok(p) => match p.parent() {
            Some(d) => d.to_path_buf(),
            None => return err(crate::util::i18n::build_no_parent_dir()),
        },
        Err(_) => return err(crate::util::i18n::build_not_file_path()),
    };

    let out_path = resolve_output_path(&base_dir, &target, "war3map.j");

    // 3. Collect ordered file list starting from the entry/trigger.
    let file_order = collect_file_order(&trigger_uri);

    // 4. Parse each file and collect fragments.
    let mut fragments = collect_fragments(&trigger_uri, &file_order, BuildMode::Jass);

    // 4b. Inline single-call-site trivial functions.
    apply_inlines(&mut fragments);

    // 4c. Fold StringHash("literal") → integer constant.
    fold_string_hash_in_fragments(&mut fragments);

    // 5. Topological sort of functions.
    let sorted_funcs = topo_sort_functions(&fragments.functions);

    // 6. Assemble output.
    let mut out = String::new();


    // Globals
    if !fragments.globals_out.is_empty() {
        out.push_str("globals\n");
        for g in &fragments.globals_out {
            out.push_str("    ");
            out.push_str(g.trim());
            out.push('\n');
        }
        out.push_str("endglobals\n\n");
    }

    // Functions in topological order.
    // If bare stmts exist and a real `main` is present, inject them at the top
    // of `main`'s body.  Otherwise emit functions as-is.
    // Every function is run through `hoist_jass_locals` so that variable
    // declarations appearing after the first instruction are moved to the top.
    let has_real_main = fragments.functions.contains_key("main");
    for fname in &sorted_funcs {
        if let Some(frag) = fragments.functions.get(fname) {
            let src = hoist_jass_locals(&frag.source);
            if fname == "main" && !fragments.bare_stmts.is_empty() {
                // Inject bare stmts right after the first line (signature).
                if let Some(first_nl) = src.find('\n') {
                    out.push_str(&src[..=first_nl]);
                    for s in &fragments.bare_stmts {
                        out.push_str("    ");
                        out.push_str(s);
                        out.push('\n');
                    }
                    out.push_str(&src[first_nl + 1..]);
                } else {
                    out.push_str(&src);
                }
            } else {
                out.push_str(&src);
            }
            out.push_str("\n\n");
        }
    }

    // Synthesized main — only when no real `main` function exists.
    if !fragments.bare_stmts.is_empty() && !has_real_main {
        out.push_str("function main takes nothing returns nothing\n");
        for s in &fragments.bare_stmts {
            out.push_str("    ");
            out.push_str(s);
            out.push('\n');
        }
        out.push_str("endfunction\n");
    }

    // 7. Write output.
    write_output(&out_path, &out, &sorted_funcs, &fragments)
}

/// Execute the AngelScript build for the file at `uri`.
///
/// Looks up `build-as` across the entire import tree, resolves the output
/// path relative to the file that contains the directive, collects all
/// JASS sources from the import tree, converts them to AngelScript syntax,
/// and emits a merged `.as` file.
///
/// Identifiers that collide with AngelScript reserved words are renamed
/// by appending a numeric suffix (`name` → `name1`, `name2`, …).
pub fn build_as(uri: &Url) -> BuildResult {
    // 1. Find build target across the whole tree.
    let (trigger_uri, target) = match find_build_setting(uri, "build-as") {
        Some(pair) => pair,
        None => return err(crate::util::i18n::build_no_setting_as()),
    };

    // 2. Resolve output path relative to the file that owns the directive.
    let base_dir = match trigger_uri.to_file_path() {
        Ok(p) => match p.parent() {
            Some(d) => d.to_path_buf(),
            None => return err(crate::util::i18n::build_no_parent_dir()),
        },
        Err(_) => return err(crate::util::i18n::build_not_file_path()),
    };

    let out_path = resolve_output_path(&base_dir, &target, "war3map.as");

    // 3. Collect ordered file list from import tree.
    let file_order = collect_file_order(&trigger_uri);

    // 4. Parse each file and collect fragments.
    let mut fragments = collect_fragments(&trigger_uri, &file_order, BuildMode::As);

    // 4b. Inline single-call-site trivial functions.
    apply_inlines(&mut fragments);

    // 4c. Fold StringHash("literal") → integer constant.
    fold_string_hash_in_fragments(&mut fragments);

    // 5. Topological sort of functions.
    let sorted_funcs = topo_sort_functions(&fragments.functions);

    // 6. Build a rename map for AS reserved-word conflicts.
    let mut all_names: Vec<&str> = Vec::new();
    for f in fragments.functions.keys() {
        all_names.push(f.as_str());
    }
    let rename_map = build_as_rename_map(&all_names);

    // 7. Convert to AngelScript and assemble output.
    let mut out = String::new();


    // Globals → top-level variable declarations.
    for g in &fragments.globals_out {
        out.push_str(&jass_var_decl_to_as(g.trim(), &rename_map));
        out.push('\n');
    }
    if !fragments.globals_out.is_empty() {
        out.push('\n');
    }

    // Functions in topological order.
    // If bare stmts exist and a real `main` is present, inject them at the
    // beginning of `main`'s body.
    let has_real_main = fragments.functions.contains_key("main");
    for fname in &sorted_funcs {
        if let Some(frag) = fragments.functions.get(fname) {
            if fname == "main" && !fragments.bare_stmts.is_empty() {
                // Convert to AS, then inject bare stmts after first line.
                let converted = jass_function_to_as(&frag.source, &rename_map);
                if let Some(first_nl) = converted.find('\n') {
                    out.push_str(&converted[..=first_nl]);
                    for s in &fragments.bare_stmts {
                        out.push_str(&jass_body_line_to_as(&format!("    {}", s), &rename_map));
                        out.push('\n');
                    }
                    out.push_str(&converted[first_nl + 1..]);
                } else {
                    out.push_str(&converted);
                }
            } else {
                out.push_str(&jass_function_to_as(&frag.source, &rename_map));
            }
            out.push_str("\n\n");
        }
    }

    // Synthesized main — only when no real `main` function exists.
    if !fragments.bare_stmts.is_empty() && !has_real_main {
        out.push_str("void main() {\n");
        for s in &fragments.bare_stmts {
            out.push_str(&jass_body_line_to_as(&format!("    {}", s), &rename_map));
            out.push('\n');
        }
        out.push_str("}\n");
    }

    write_output(&out_path, &out, &sorted_funcs, &fragments)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Search for a build setting `key` in `//entry` files within the connected
/// component of the import tree. Returns `(uri_of_entry_file, setting_value)`.
///
/// Build directives (`//set build-jass`, `//set build-as`) are only honoured
/// in files that carry the `//entry` directive.  Non-entry files that contain
/// these settings are diagnosed as errors and are never used as build origins.
/// The build always starts from the entry file and follows its imports.
fn find_build_setting(uri: &Url, key: &str) -> Option<(Url, String)> {
    // Check the current file first.
    if let Some(fs) = FILE_STORE.get(uri) {
        if fs.file_symbols.is_entry {
            if let Some(v) = fs.file_symbols.file_settings.get(key) {
                return Some((uri.clone(), v.clone()));
            }
        }
    }
    // Search the connected component.
    let component = IMPORT_GRAPH.connected_component(uri);
    for u in &component {
        if let Some(fs) = FILE_STORE.get(u) {
            if fs.file_symbols.is_entry {
                if let Some(v) = fs.file_symbols.file_settings.get(key) {
                    return Some((u.clone(), v.clone()));
                }
            }
        }
    }
    None
}

/// Check whether `key` exists in any file of the connected component.
pub fn has_build_setting(uri: &Url, key: &str) -> bool {
    find_build_setting(uri, key).is_some()
}

fn err(msg: &str) -> BuildResult {
    BuildResult {
        ok: false,
        path: String::new(),
        message: msg.to_string(),
    }
}

fn write_output(
    out_path: &Path,
    out: &str,
    sorted_funcs: &[String],
    fragments: &Fragments,
) -> BuildResult {
    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(out_path, out) {
        Ok(_) => BuildResult {
            ok: true,
            path: out_path.display().to_string(),
            message: crate::util::i18n::build_ok(
                fragments.globals_out.len(),
                sorted_funcs.len(),
                fragments.bare_stmts.len(),
            ),
        },
        Err(e) => err(&crate::util::i18n::build_write_failed(
            &out_path.display().to_string(),
            &e.to_string(),
        )),
    }
}

/// Inline candidate info: a function with no parameters whose body is a
/// single `return expr` statement.
#[derive(Clone)]
struct InlineCandidate {
    /// The text of the return expression.
    expr_text: String,
    /// Whether the expression is compound (binary/unary) and needs wrapping
    /// in parentheses when inlined into a sub-expression context.
    is_compound: bool,
}

#[allow(dead_code)]
struct FuncFragment {
    name: String,
    source: String,
    callees: HashSet<String>,
    /// If this function is an inline candidate, stores the info.
    inline_expr: Option<InlineCandidate>,
}

/// Collected fragments from all files in the import tree.
struct Fragments {
    globals_out: Vec<String>,
    functions: HashMap<String, FuncFragment>,
    bare_stmts: Vec<String>,
}

// ─── JASS emitters — reconstruct clean single-line JASS from AST/CST ─────────

/// Collapse a CST node's text to a single line (all whitespace → single space).
fn flatten(src: &str, node: &tree_sitter::Node) -> String {
    let text = &src[node.start_byte()..node.end_byte()];
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Read raw identifier text.
fn id_text(src: &str, id: &Id) -> String {
    src[id.node.start_byte()..id.node.end_byte()].to_string()
}

/// Flatten an expression to a single-line string.
///
/// When `for_as` is `true`, the expression is recursively emitted with
/// parentheses inserted where JASS and AngelScript operator precedence
/// differs (specifically: `or` operands of `and` are wrapped).
fn emit_expr(src: &str, expr: &Expr, for_as: bool) -> String {
    if for_as {
        return emit_expr_as(src, expr);
    }
    flatten(src, expr.cst_node())
}

/// Extract the operator text from a binary expression by looking at the
/// source between the left and right operand CST spans.
fn binary_op_text(src: &str, left: &Expr, right: &Expr) -> String {
    let left_end = left.cst_node().end_byte();
    let right_start = right.cst_node().start_byte();
    if left_end < right_start {
        src[left_end..right_start].trim().to_string()
    } else {
        String::new()
    }
}

/// Check whether an expression is a binary `or` expression.
fn is_or_expr(src: &str, expr: &Expr) -> bool {
    if let Expr::Binary { left, right, .. } = expr {
        binary_op_text(src, left, right) == "or"
    } else {
        false
    }
}

/// Recursively emit an expression for AngelScript output.
///
/// In JASS, `or` has **higher** precedence than `and` (binds tighter).
/// In AS / C++, `and` (`&&`) has higher precedence than `or` (`||`).
/// Therefore, when a child of `and` is an `or` expression, we wrap it in
/// parentheses so that AS interprets it with the same semantics as JASS.
fn emit_expr_as(src: &str, expr: &Expr) -> String {
    match expr {
        Expr::Binary { node: _, left, right } => {
            let op = binary_op_text(src, left, right);
            let left_str = if op == "and" && is_or_expr(src, left) {
                format!("({})", emit_expr_as(src, left))
            } else {
                emit_expr_as(src, left)
            };
            let right_str = if op == "and" && is_or_expr(src, right) {
                format!("({})", emit_expr_as(src, right))
            } else {
                emit_expr_as(src, right)
            };
            format!("{} {} {}", left_str, op, right_str)
        }
        Expr::Unary { node, operand } => {
            let op_end = operand.cst_node().start_byte();
            let op_text = src[node.start_byte()..op_end].trim();
            format!("{} {}", op_text, emit_expr_as(src, operand))
        }
        Expr::Parens { inner, .. } => {
            format!("({})", emit_expr_as(src, inner))
        }
        Expr::Call(fc) => {
            let name = fc.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
            let args: Vec<String> = fc.args.iter().map(|a| emit_expr_as(src, a)).collect();
            format!("{}({})", name, args.join(", "))
        }
        Expr::Index { array, index, .. } => {
            format!("{}[{}]", emit_expr_as(src, array), emit_expr_as(src, index))
        }
        Expr::FuncRef(id) => {
            format!("function {}", id_text(src, id))
        }
        Expr::Id(id) => id_text(src, id),
        Expr::Literal(node) => flatten(src, node),
    }
}

/// `set VAR[INDEX] = VALUE` — always emits the `set` keyword.
fn emit_set(src: &str, s: &SetStmt, for_as: bool) -> String {
    let var = s.variable.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
    let idx = match &s.index {
        Some(e) => format!("[{}]", emit_expr(src, e, for_as)),
        None => String::new(),
    };
    let val = s.value.as_ref().map(|e| emit_expr(src, e, for_as)).unwrap_or_default();
    format!("set {}{} = {}", var, idx, val)
}

/// `call FUNC(ARGS)` — always emits the `call` keyword.
fn emit_call(src: &str, c: &CallStmt, for_as: bool) -> String {
    match &c.func {
        Some(fc) => {
            let name = fc.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
            let args: Vec<String> = fc.args.iter().map(|a| emit_expr(src, a, for_as)).collect();
            format!("call {}({})", name, args.join(", "))
        }
        None => "call ???()".to_string(),
    }
}

/// `return [VALUE]`
fn emit_return(src: &str, r: &ReturnStmt, for_as: bool) -> String {
    match &r.value {
        Some(e) => format!("return {}", emit_expr(src, e, for_as)),
        None => "return".to_string(),
    }
}

/// `exitwhen COND`
fn emit_exitwhen(src: &str, e: &ExitwhenStmt, for_as: bool) -> String {
    let cond = e.condition.as_ref().map(|c| emit_expr(src, c, for_as)).unwrap_or_default();
    format!("exitwhen {}", cond)
}

/// `local TYPE NAME [= VALUE]`
fn emit_local(src: &str, l: &LocalDecl, for_as: bool) -> String {
    let type_name = l.type_id.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "integer".to_string());
    let name = l.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
    match &l.value {
        Some(e) => format!("local {} {} = {}", type_name, name, emit_expr(src, e, for_as)),
        None => format!("local {} {}", type_name, name),
    }
}

/// `[constant] TYPE [array] NAME [= VALUE], ...`
fn emit_var(src: &str, v: &VarStmt, for_as: bool) -> String {
    let type_name = v.type_id.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "integer".to_string());
    let mut prefix = String::new();
    if v.is_constant { prefix.push_str("constant "); }
    prefix.push_str(&type_name);
    if v.is_array { prefix.push_str(" array"); }
    let decls: Vec<String> = v.decls.iter().map(|d| {
        let name = d.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
        match &d.value {
            Some(e) => format!("{} = {}", name, emit_expr(src, e, for_as)),
            None => name,
        }
    }).collect();
    format!("{} {}", prefix, decls.join(", "))
}

/// Emit a list of AST statements as properly formatted lines.
///
/// Each simple statement is one line; compound statements (`if`, `loop`)
/// expand into multiple lines with correct indentation.
fn emit_body(src: &str, stmts: &[Statement], indent: &str, for_as: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for stmt in stmts {
        match stmt {
            Statement::Set(s) => lines.push(format!("{}{}", indent, emit_set(src, s, for_as))),
            Statement::Call(c) => lines.push(format!("{}{}", indent, emit_call(src, c, for_as))),
            Statement::Return(r) => lines.push(format!("{}{}", indent, emit_return(src, r, for_as))),
            Statement::Exitwhen(e) => lines.push(format!("{}{}", indent, emit_exitwhen(src, e, for_as))),
            Statement::Local(l) => lines.push(format!("{}{}", indent, emit_local(src, l, for_as))),
            Statement::VarStmt(v) => lines.push(format!("{}local {}", indent, emit_var(src, v, for_as))),
            Statement::If(i) => lines.extend(emit_if(src, i, indent, for_as)),
            Statement::Loop(l) => {
                let inner = format!("{}    ", indent);
                lines.push(format!("{}loop", indent));
                lines.extend(emit_body(src, &l.body, &inner, for_as));
                lines.push(format!("{}endloop", indent));
            }
            _ => {}
        }
    }
    lines
}

/// Emit a single CST node as statement lines (used inside CST-based if/loop walkers).
fn emit_cst_node(src: &str, node: &tree_sitter::Node, kind: Kind, indent: &str, for_as: bool) -> Vec<String> {
    match kind {
        Kind::SetStatement | Kind::CallStatement | Kind::ReturnStatement
        | Kind::ExitwhenStatement | Kind::LocalStatement | Kind::VarStmt => {
            vec![format!("{}{}", indent, flatten(src, node))]
        }
        Kind::IfStatement => emit_if_cst(src, node, indent, for_as),
        Kind::LoopStatement => emit_loop_cst(src, node, indent, for_as),
        _ => vec![],
    }
}

/// Emit an `if`/`elseif`/`else`/`endif` block from the AST.
fn emit_if(src: &str, i: &IfStmt, indent: &str, for_as: bool) -> Vec<String> {
    let inner = format!("{}    ", indent);
    let mut lines = Vec::new();

    // First branch: `if COND then ...`
    let cond = i.condition.as_ref()
        .map(|c| emit_expr(src, c, for_as))
        .unwrap_or_default();
    lines.push(format!("{}if {} then", indent, cond));
    lines.extend(emit_body(src, &i.body, &inner, for_as));

    // Subsequent branches: `elseif COND then ...` / `else ...`
    for branch in &i.branches {
        if let Some(ref cond) = branch.condition {
            lines.push(format!("{}elseif {} then", indent, emit_expr(src, cond, for_as)));
        } else {
            lines.push(format!("{}else", indent));
        }
        lines.extend(emit_body(src, &branch.body, &inner, for_as));
    }

    lines.push(format!("{}endif", indent));
    lines
}

/// Walk a `loop_statement` CST node and emit properly formatted lines.
fn emit_loop_cst(src: &str, node: &tree_sitter::Node, indent: &str, for_as: bool) -> Vec<String> {
    let mut lines = Vec::new();
    let inner = format!("{}    ", indent);

    lines.push(format!("{}loop", indent));

    for idx in 0..node.child_count() as u32 {
        let child = match node.child(idx) {
            Some(c) => c,
            None => continue,
        };
        if !child.is_named() {
            continue;
        }
        if let Ok(nk) = Kind::try_from(child.kind_id()) {
            lines.extend(emit_cst_node(src, &child, nk, &inner, for_as));
        }
    }

    lines.push(format!("{}endloop", indent));
    lines
}

/// Walk an `if_statement` CST node and emit properly formatted lines.
///
/// CST structure (flat children):
///   `if` COND `then` STMTS [`elseif` COND `then` STMTS]* [`else` STMTS] `endif`
fn emit_if_cst(src: &str, node: &tree_sitter::Node, indent: &str, for_as: bool) -> Vec<String> {
    let inner = format!("{}    ", indent);
    let mut lines = Vec::new();

    // State machine phases matching the CST layout.
    enum Phase { IfCond, FirstBody, ElseifCond, ElseifBody, ElseBody }
    let mut phase = Phase::IfCond;
    let mut cond_parts: Vec<String> = Vec::new();

    for idx in 0..node.child_count() as u32 {
        let child = match node.child(idx) {
            Some(c) => c,
            None => continue,
        };

        let kind = Kind::try_from(child.kind_id()).ok();

        match (&phase, kind) {
            // `if` keyword — skip.
            (Phase::IfCond, Some(Kind::If)) => {}
            // Condition expression(s) before `then`.
            (Phase::IfCond, Some(Kind::Then)) => {
                let cond = cond_parts.join(" ");
                lines.push(format!("{}if {} then", indent, cond));
                cond_parts.clear();
                phase = Phase::FirstBody;
            }
            (Phase::IfCond, _) => {
                if child.is_named() {
                    cond_parts.push(flatten(src, &child));
                }
            }

            // First body — statements between `then` and `elseif`/`else`/`endif`.
            (Phase::FirstBody, Some(Kind::Elseif)) => {
                cond_parts.clear();
                phase = Phase::ElseifCond;
            }
            (Phase::FirstBody, Some(Kind::Else)) => {
                lines.push(format!("{}else", indent));
                phase = Phase::ElseBody;
            }
            (Phase::FirstBody, Some(Kind::Endif)) => {
                lines.push(format!("{}endif", indent));
            }
            (Phase::FirstBody, _) => {
                if child.is_named() {
                    if let Ok(nk) = Kind::try_from(child.kind_id()) {
                        lines.extend(emit_cst_node(src, &child, nk, &inner, for_as));
                    }
                }
            }

            // `elseif` condition.
            (Phase::ElseifCond, Some(Kind::Then)) => {
                let cond = cond_parts.join(" ");
                lines.push(format!("{}elseif {} then", indent, cond));
                cond_parts.clear();
                phase = Phase::ElseifBody;
            }
            (Phase::ElseifCond, _) => {
                if child.is_named() {
                    cond_parts.push(flatten(src, &child));
                }
            }

            // Elseif body.
            (Phase::ElseifBody, Some(Kind::Elseif)) => {
                cond_parts.clear();
                phase = Phase::ElseifCond;
            }
            (Phase::ElseifBody, Some(Kind::Else)) => {
                lines.push(format!("{}else", indent));
                phase = Phase::ElseBody;
            }
            (Phase::ElseifBody, Some(Kind::Endif)) => {
                lines.push(format!("{}endif", indent));
            }
            (Phase::ElseifBody, _) => {
                if child.is_named() {
                    if let Ok(nk) = Kind::try_from(child.kind_id()) {
                        lines.extend(emit_cst_node(src, &child, nk, &inner, for_as));
                    }
                }
            }

            // Else body.
            (Phase::ElseBody, Some(Kind::Endif)) => {
                lines.push(format!("{}endif", indent));
            }
            (Phase::ElseBody, _) => {
                if child.is_named() {
                    if let Ok(nk) = Kind::try_from(child.kind_id()) {
                        lines.extend(emit_cst_node(src, &child, nk, &inner, for_as));
                    }
                }
            }
        }
    }

    lines
}

/// Emit `function NAME takes PARAMS returns TYPE`
fn emit_func_sig(src: &str, f: &FunctionDecl) -> String {
    let name = f.name.as_ref().map(|id| id_text(src, id)).unwrap_or_default();
    let params = if f.params.is_empty() {
        "nothing".to_string()
    } else {
        f.params
            .iter()
            .map(|p| {
                let t = p.type_id.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "integer".to_string());
                let n = p.name.as_ref().map(|id| id_text(src, id)).unwrap_or_else(|| "_".to_string());
                format!("{} {}", t, n)
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let ret = f
        .return_type
        .as_ref()
        .map(|id| id_text(src, id))
        .unwrap_or_else(|| "nothing".to_string());
    format!("function {} takes {} returns {}", name, params, ret)
}

/// Emit a complete function: signature + indented body + endfunction.
fn emit_function(src: &str, f: &FunctionDecl, for_as: bool) -> String {
    let sig = emit_func_sig(src, f);
    let body_lines = emit_body(src, &f.body, "    ", for_as);
    let mut out = sig;
    out.push('\n');
    for line in &body_lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("endfunction");
    out
}

/// Test-only wrapper for [`emit_function`].
#[cfg(test)]
pub fn emit_function_text(src: &str, f: &FunctionDecl) -> String {
    emit_function(src, f, false)
}

/// Test-only wrapper for [`emit_function`] in AS mode (with precedence fix).
#[cfg(test)]
pub fn emit_function_text_as(src: &str, f: &FunctionDecl) -> String {
    emit_function(src, f, true)
}

/// Test-only wrapper for [`emit_var`] in AS mode.
#[cfg(test)]
pub fn emit_var_text_as(src: &str, v: &VarStmt) -> String {
    emit_var(src, v, true)
}

/// Test-only wrapper for [`hoist_jass_locals`].
#[cfg(test)]
pub fn hoist_jass_locals_text(source: &str) -> String {
    hoist_jass_locals(source)
}

/// Test-only: detect an inline candidate from source code.
#[cfg(test)]
pub fn detect_inline_candidate_text(src: &str) -> Option<(String, bool)> {
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
                if let Some(ic) = detect_inline_candidate(src, &f.body, false) {
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

/// Resolve `<path>` relative to `base_dir`. If `<path>` looks like a directory
/// (ends with `/` or `\` or has no extension), append `default_file`.
fn resolve_output_path(base_dir: &Path, target: &str, default_file: &str) -> PathBuf {
    let raw = target.replace('\\', "/");
    let p = Path::new(&raw);

    let resolved = if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    };

    // If it looks like a directory, append default filename.
    if raw.ends_with('/') || resolved.extension().is_none() {
        resolved.join(default_file)
    } else {
        resolved
    }
}

/// Collect the ordered list of file URIs to process for a build.
///
/// The `trigger_uri` is the `//entry` file that owns the `//set build-*`
/// directive.  The file order is the set of files transitively imported by
/// `trigger_uri` (forward-only BFS), with the entry file itself appended
/// last so its bare top-level statements end up in `main`.
/// Files not reachable from the entry are excluded (tree-shaking).
fn collect_file_order(trigger_uri: &Url) -> Vec<Url> {
    let mut deps = IMPORT_GRAPH.dependencies(trigger_uri);
    // Put the trigger file last (its bare statements go into main).
    deps.push(trigger_uri.clone());
    // Deduplicate while preserving order.
    let mut seen = HashSet::new();
    deps.retain(|u| seen.insert(u.clone()));
    deps
}

/// Read file source: from ROPE_MAP if open, otherwise from disk.
fn read_file_source(uri: &Url) -> Option<String> {
    use crate::util::roper::uri_map::ROPE_MAP;
    if let Some(rope) = ROPE_MAP.get(uri) {
        return Some(rope.to_string());
    }
    let path = uri.to_file_path().ok()?;
    std::fs::read_to_string(&path).ok()
}

/// Parse all files and collect fragments (types, globals, functions, bare stmts).
///
/// - **Frozen (`//import!`) files** are skipped entirely — they are
///   engine-provided / read-only and never belong in the build output.
/// - **`type` and `native` declarations** are never collected.
fn collect_fragments(_trigger_uri: &Url, file_order: &[Url], mode: BuildMode) -> Fragments {
    let for_as = mode == BuildMode::As;
    let mut globals_out = Vec::<String>::new();
    let mut functions: HashMap<String, FuncFragment> = HashMap::new();
    let mut bare_stmts = Vec::<String>::new();

    for file_uri in file_order {
        // Frozen files are skipped entirely in every build mode.
        if is_uri_frozen(file_uri) {
            continue;
        }

        let src = match read_file_source(file_uri) {
            Some(s) => s,
            None => continue,
        };

        let mut parser = tree_sitter::Parser::new();
        if parser
            .set_language(&tree_sitter_jass::language().into())
            .is_err()
        {
            continue;
        }
        let tree = match parser.parse(&src, None) {
            Some(t) => t,
            None => continue,
        };

        let mut ast = build_ast(tree.root_node());
        let src_bytes = src.as_bytes().to_vec();
        rewrite_imports(&mut ast, &src_bytes);

        for item in &ast.items {
            match item {
                // Type declarations (`type X extends Y`) and native
                // declarations (`native Foo takes ...`) are never emitted
                // in either JASS or AS build output — they are
                // engine-provided and have no runtime representation.
                Statement::Type(_) | Statement::Native(_) => {}
                Statement::Globals(g) => {
                    for v in &g.vars {
                        globals_out.push(emit_var(&src, v, for_as));
                    }
                }
                Statement::Function(f) => {
                    let fname = f
                        .name
                        .as_ref()
                        .map(|id| id_text(&src, id))
                        .unwrap_or_default();
                    if !fname.is_empty() {
                        let callees: HashSet<String> = FILE_STORE
                            .get(file_uri)
                            .map(|fs| {
                                fs.file_symbols.functions
                                    .iter()
                                    .find(|ff| ff.name == fname)
                                    .map(|ff| ff.callees.clone())
                                    .unwrap_or_default()
                            })
                            .unwrap_or_default();

                        // Detect inline candidate: takes nothing + single `return expr`.
                        let inline_expr = if f.params.is_empty() {
                            detect_inline_candidate(&src, &f.body, for_as)
                        } else {
                            None
                        };

                        functions.insert(
                            fname.clone(),
                            FuncFragment {
                                name: fname,
                                source: emit_function(&src, f, for_as),
                                callees,
                                inline_expr,
                            },
                        );
                    }
                }
                Statement::VarStmt(v) => {
                    globals_out.push(emit_var(&src, v, for_as));
                }
                Statement::Set(s) => {
                    bare_stmts.push(emit_set(&src, s, for_as));
                }
                Statement::Call(c) => {
                    bare_stmts.push(emit_call(&src, c, for_as));
                }
                Statement::If(i) => {
                    bare_stmts.extend(emit_if_cst(&src, &i.node, "", for_as));
                }
                Statement::Loop(l) => {
                    bare_stmts.push("loop".to_string());
                    bare_stmts.extend(emit_body(&src, &l.body, "    ", for_as));
                    bare_stmts.push("endloop".to_string());
                }
                // Skip imports, set directives, comments, errors, locals, returns, exitwhens.
                _ => {}
            }
        }
    }

    Fragments {
        globals_out,
        functions,
        bare_stmts,
    }
}

// ─── Function inlining ──────────────────────────────────────────────────────

/// Detect whether a function body is a single `return expr` and, if so,
/// return the [`InlineCandidate`] with the expression text and compoundness.
fn detect_inline_candidate(
    src: &str,
    body: &[Statement],
    for_as: bool,
) -> Option<InlineCandidate> {
    // Body must contain exactly one statement: `return <expr>`.
    if body.len() != 1 {
        return None;
    }
    if let Statement::Return(r) = &body[0] {
        let expr = r.value.as_ref()?;
        let expr_text = emit_expr(src, expr, for_as);
        let is_compound = matches!(expr, Expr::Binary { .. } | Expr::Unary { .. });
        Some(InlineCandidate { expr_text, is_compound })
    } else {
        None
    }
}

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
fn is_top_level_call(source: &str, call_start: usize, call_end: usize) -> bool {
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

/// Replace `NAME()` calls in `source` with the inlined expression.
///
/// - Top-level calls (sole expression in a `call`/`return`/`set`/`if`/etc.)
///   get the expression as-is.
/// - Nested calls inside larger expressions get the expression wrapped in
///   parentheses when it is compound (binary/unary).
fn inline_call_in_source(source: &str, func_name: &str, candidate: &InlineCandidate) -> String {
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

/// Inline functions that take nothing, have a single `return expr` body,
/// and are called exactly once across the entire build output.
///
/// Inlined functions are removed from the function map so they are not
/// emitted in the final output.
fn apply_inlines(fragments: &mut Fragments) {
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

/// Fold `StringHash(expr)` → integer constant in all fragments.
///
/// First collects compile-time constant values (`constant string`, `constant integer`)
/// from globals, then evaluates `StringHash(...)` argument expressions.
/// Also folds string expressions that appear in integer parameter positions.
fn fold_string_hash_in_fragments(fragments: &mut Fragments) {
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

/// Simple topological sort of functions by callees using DFS.
fn topo_sort_functions(functions: &HashMap<String, FuncFragment>) -> Vec<String> {
    let mut visited = HashSet::new();
    let mut order = Vec::new();

    fn dfs(
        name: &str,
        functions: &HashMap<String, FuncFragment>,
        visited: &mut HashSet<String>,
        order: &mut Vec<String>,
    ) {
        if visited.contains(name) {
            return;
        }
        visited.insert(name.to_string());

        if let Some(frag) = functions.get(name) {
            for callee in &frag.callees {
                if functions.contains_key(callee) {
                    dfs(callee, functions, visited, order);
                }
            }
        }
        order.push(name.to_string());
    }

    // Alphabetical seed order for determinism.
    let mut names: Vec<&String> = functions.keys().collect();
    names.sort();
    for name in names {
        dfs(name, functions, &mut visited, &mut order);
    }
    order
}

// ─── JASS → AngelScript conversion ──────────────────────────────────────────

/// AngelScript reserved words that cannot be used as identifiers.
const AS_RESERVED: &[&str] = &[
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
fn build_as_rename_map(names: &[&str]) -> HashMap<String, String> {
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
fn as_rename(name: &str, rename_map: &HashMap<String, String>) -> String {
    rename_map
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.to_string())
}

/// Map a JASS type name to an AS type name.
fn jass_type_to_as_type(t: &str) -> &str {
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
fn default_for_as_type(as_type: &str) -> &str {
    match as_type {
        "int" => "0",
        "float" => "0",
        "bool" => "false",
        "string" => "\"\"",
        _ => "null",
    }
}

/// Default literal for a JASS type (used for hoisted local declarations).
///
/// - `integer` → `0`, `real` → `0`, `boolean` → `false`,
///   `string` → `""`, everything else → `null`.
fn default_for_jass_type(jass_type: &str) -> &str {
    match jass_type {
        "integer" => "0",
        "real" => "0",
        "boolean" => "false",
        "string" => "\"\"",
        _ => "null",
    }
}

/// Extract type / name pairs from a declaration line (JASS-side hoisting).
///
/// Returns `Vec<(jass_type, name, is_array)>`.
fn extract_jass_hoisted_vars(trimmed: &str) -> Vec<(String, String, bool)> {
    let mut t = trimmed;
    t = t.strip_prefix("local ").unwrap_or(t);
    t = t.strip_prefix("constant ").unwrap_or(t);

    let mut parts = t.splitn(2, ' ');
    let type_name = parts.next().unwrap_or("integer").to_string();
    let rest = parts.next().unwrap_or("");

    let (is_array, rest) = if let Some(r) = rest.strip_prefix("array ") {
        (true, r)
    } else {
        (false, rest)
    };

    rest.split(',')
        .filter_map(|decl| {
            let name = decl.trim().split('=').next()?.trim()
                .split_whitespace().next()?;
            if name.is_empty() { return None; }
            Some((type_name.clone(), name.to_string(), is_array))
        })
        .collect()
}

/// Convert a hoisted JASS variable declaration into `set NAME = VALUE` lines.
///
/// If there is no initialiser the line is omitted (the hoisted `local`
/// at the top is sufficient).
fn jass_var_decl_to_set_assignments(line: &str) -> Vec<String> {
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
            let name = decl[..eq_pos].trim();
            let value = decl[eq_pos + 1..].trim();
            Some(format!("{}set {} = {}", indent, name, value))
        })
        .collect()
}

/// Hoist late local declarations in a JASS function source to the top.
///
/// In JASS, `local` declarations must appear before any other statement.
/// Any variable declaration found after the first non-declaration
/// statement is moved to the top of the function body (with the type's
/// default value), and the original site becomes a plain `set` assignment.
fn hoist_jass_locals(source: &str) -> String {
    let mut lines_iter = source.lines();
    let sig = match lines_iter.next() {
        Some(l) => l,
        None => return source.to_string(),
    };

    let body_lines: Vec<&str> = lines_iter.collect();

    // ── Pass 1: find declarations that appear after the first instruction ──
    // Track all declared variable names to avoid duplicate hoisted locals.
    let mut declared_names: HashSet<String> = HashSet::new();
    let mut seen_instruction = false;
    let mut hoisted: Vec<(String, String, bool)> = Vec::new();
    let mut has_late_decls = false;

    for line in &body_lines {
        let t = line.trim();
        if t.is_empty() || t == "endfunction" {
            continue;
        }
        if is_var_decl_line(t) {
            let vars = extract_jass_hoisted_vars(t);
            if seen_instruction {
                has_late_decls = true;
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

    if !has_late_decls {
        return source.to_string();
    }

    let mut out = String::from(sig);

    // Emit hoisted declarations right after the signature.
    for (type_name, var_name, is_array) in &hoisted {
        out.push('\n');
        if *is_array {
            out.push_str(&format!("    local {} array {}", type_name, var_name));
        } else {
            out.push_str(&format!(
                "    local {} {} = {}",
                type_name,
                var_name,
                default_for_jass_type(type_name),
            ));
        }
    }

    // ── Pass 2: emit body, converting hoisted decls to `set` assignments ──
    seen_instruction = false;
    for line in &body_lines {
        let t = line.trim();
        if t == "endfunction" {
            out.push('\n');
            out.push_str("endfunction");
            continue;
        }
        if t.is_empty() {
            out.push('\n');
            continue;
        }

        if is_var_decl_line(t) && seen_instruction {
            // Hoisted — emit only the set assignment(s), if any.
            for a in jass_var_decl_to_set_assignments(line) {
                out.push('\n');
                out.push_str(&a);
            }
        } else {
            if !is_var_decl_line(t) {
                seen_instruction = true;
            }
            out.push('\n');
            out.push_str(line);
        }
    }

    out
}

/// Determine whether a trimmed body line is a variable declaration.
///
/// Returns `true` for lines like `local TYPE NAME`, `TYPE NAME = …`,
/// `constant TYPE array NAME`, etc.  Returns `false` for known
/// statement keywords (`set`, `call`, `return`, `exitwhen`, `if`,
/// `loop`, etc.) and control-flow markers.
fn is_var_decl_line(trimmed: &str) -> bool {
    if trimmed.starts_with("local ") || trimmed.starts_with("constant ") {
        return true;
    }
    if trimmed.is_empty()
        || trimmed.starts_with("set ")
        || trimmed.starts_with("call ")
        || trimmed.starts_with("return")
        || trimmed.starts_with("exitwhen ")
        || trimmed == "loop"
        || trimmed == "endloop"
        || trimmed.starts_with("if ")
        || trimmed.starts_with("elseif ")
        || trimmed == "else"
        || trimmed == "endif"
        || trimmed == "endfunction"
    {
        return false;
    }
    // Must have at least TYPE + NAME (two whitespace-separated tokens).
    trimmed.split_whitespace().count() >= 2
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
fn jass_function_to_as(source: &str, rename_map: &HashMap<String, String>) -> String {
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
            out.push_str(&format!("    array<{}> {};", as_type, as_name));
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
fn jass_var_decl_to_as(decl: &str, rename_map: &HashMap<String, String>) -> String {
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
        format!("array<{}> {}", as_type, apply_rename_to_line(rest, rename_map))
    } else {
        format!("{} {}", as_type, apply_rename_to_line(rest, rename_map))
    }
}

/// Apply renames to identifiers in a line of code.
/// Simple word-boundary replacement.
fn apply_rename_to_line(line: &str, rename_map: &HashMap<String, String>) -> String {
    if rename_map.is_empty() {
        return line.to_string();
    }
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

