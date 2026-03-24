//! Owned IR — tree-sitter-lifetime-free representation.
//!
//! These types form the backbone of the build pipeline: source AST nodes are
//! converted into these owned types so that the rest of the pipeline (rendering,
//! inlining, map-data augmentation) can operate without holding borrows on the
//! tree-sitter trees.
//!
//! The IR is currently JASS-flavoured (e.g. `FuncRef` corresponds to
//! `function name`).  When the direct binary→AS path is implemented, the IR
//! may gain AS-specific variants or a parallel set of types.

use std::collections::{HashMap, HashSet};

// ─── Inline candidate ────────────────────────────────────────────────────────

/// Inline candidate info: a function with no parameters whose body is a
/// single `return expr` statement.
#[derive(Clone)]
pub(super) struct InlineCandidate {
    /// The text of the return expression.
    pub expr_text: String,
    /// Whether the expression is compound (binary/unary) and needs wrapping
    /// in parentheses when inlined into a sub-expression context.
    pub is_compound: bool,
}

// ─── Fragments ───────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub(super) struct FuncFragment {
    pub name: String,
    pub source: String,
    pub callees: HashSet<String>,
    /// If this function is an inline candidate, stores the info.
    pub inline_expr: Option<InlineCandidate>,
}

/// Collected fragments from all files in the import tree.
pub(super) struct Fragments {
    pub globals_out: Vec<String>,
    pub functions: HashMap<String, FuncFragment>,
    pub bare_stmts: Vec<String>,
}

// ─── Expressions ─────────────────────────────────────────────────────────────

/// Owned expression node.
#[derive(Debug, Clone)]
pub(super) enum IRExpr {
    /// Literal value: number, string, rawcode, boolean, `null`.
    Literal(String),
    /// Variable / constant identifier.
    Id(String),
    /// Function call: `name(args…)`.
    Call { name: String, args: Vec<IRExpr> },
    /// Function reference: `function name`.
    FuncRef(String),
    /// Binary: `left OP right`.
    Binary { left: Box<IRExpr>, op: String, right: Box<IRExpr> },
    /// Unary: `OP operand`.
    Unary { op: String, operand: Box<IRExpr> },
    /// Parenthesized: `(inner)`.
    Parens(Box<IRExpr>),
    /// Array index: `array[index]`.
    Index { array: Box<IRExpr>, index: Box<IRExpr> },
    /// Type cast: `type_name(inner)` — used in AS for typed reads from `table`.
    /// In JASS rendering this is transparent: emits `type_name(inner)` which
    /// looks like a function call and passes through the text pipeline safely.
    Cast { type_name: String, inner: Box<IRExpr> },
}

/// Escape a string for use inside a JASS/AS string literal.
fn jass_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

impl IRExpr {
    #[allow(dead_code)]
    pub fn lit(s: impl Into<String>) -> Self { IRExpr::Literal(s.into()) }
    pub fn id(s: impl Into<String>) -> Self { IRExpr::Id(s.into()) }
    pub fn call(name: impl Into<String>, args: Vec<IRExpr>) -> Self {
        IRExpr::Call { name: name.into(), args }
    }
    pub fn binary(left: IRExpr, op: impl Into<String>, right: IRExpr) -> Self {
        IRExpr::Binary { left: Box::new(left), op: op.into(), right: Box::new(right) }
    }
    pub fn int(v: impl std::fmt::Display) -> Self { IRExpr::Literal(format!("{}", v)) }
    pub fn float1(v: f32) -> Self { IRExpr::Literal(format!("{:.1}", v)) }
    pub fn float3(v: f32) -> Self { IRExpr::Literal(format!("{:.3}", v)) }
    pub fn string(s: &str) -> Self { IRExpr::Literal(format!("\"{}\"", jass_escape(s))) }
    pub fn rawcode(s: &str) -> Self { IRExpr::Literal(format!("'{}'", s)) }
    pub fn bool_val(b: bool) -> Self { IRExpr::Literal(if b { "true" } else { "false" }.into()) }
    pub fn null() -> Self { IRExpr::Literal("null".into()) }
}

// ─── Statements ──────────────────────────────────────────────────────────────

/// One variable initializer in a `VarDecl`.
#[derive(Debug, Clone)]
pub(super) struct IRVarInit {
    pub name: String,
    pub value: Option<IRExpr>,
}

/// One branch (`elseif` / `else`) in an `If` statement.
#[derive(Debug, Clone)]
pub(super) struct IRBranch {
    /// `Some` for `elseif`, `None` for `else`.
    pub condition: Option<IRExpr>,
    pub body: Vec<IRStmt>,
}

/// Owned statement node.
#[derive(Debug, Clone)]
pub(super) enum IRStmt {
    /// `local TYPE [array] NAME [= VALUE]`
    Local { type_name: String, is_array: bool, name: String, value: Option<IRExpr> },
    /// `set VAR[INDEX] = VALUE`
    Set { var: String, index: Option<IRExpr>, value: IRExpr },
    /// `call NAME(ARGS…)`
    Call { name: String, args: Vec<IRExpr> },
    /// `return [VALUE]`
    Return(Option<IRExpr>),
    /// `exitwhen COND`
    Exitwhen(IRExpr),
    /// `if COND then … [elseif …] [else …] endif`
    If { condition: IRExpr, body: Vec<IRStmt>, branches: Vec<IRBranch> },
    /// `loop … endloop`
    Loop(Vec<IRStmt>),
    /// Global variable declaration: `[constant] TYPE [array] NAME [= VALUE], …`
    VarDecl { is_constant: bool, is_array: bool, type_name: String, decls: Vec<IRVarInit> },
}

impl IRStmt {
    pub fn call(name: impl Into<String>, args: Vec<IRExpr>) -> Self {
        IRStmt::Call { name: name.into(), args }
    }
    pub fn set(var: impl Into<String>, value: IRExpr) -> Self {
        IRStmt::Set { var: var.into(), index: None, value }
    }
    #[allow(dead_code)]
    pub fn set_idx(var: impl Into<String>, index: IRExpr, value: IRExpr) -> Self {
        IRStmt::Set { var: var.into(), index: Some(index), value }
    }
    pub fn local(type_name: impl Into<String>, name: impl Into<String>) -> Self {
        IRStmt::Local { type_name: type_name.into(), is_array: false, name: name.into(), value: None }
    }
    #[allow(dead_code)]
    pub fn local_init(type_name: impl Into<String>, name: impl Into<String>, value: IRExpr) -> Self {
        IRStmt::Local { type_name: type_name.into(), is_array: false, name: name.into(), value: Some(value) }
    }
}

// ─── Functions & top-level IR ────────────────────────────────────────────────

/// Owned function representation.
pub(super) struct IRFunc {
    pub name: String,
    pub params: Vec<(String, String)>,  // (type_name, param_name)
    pub return_type: String,            // "nothing" when void
    pub body: Vec<IRStmt>,
    pub callees: HashSet<String>,
    pub inline_expr: Option<InlineCandidate>,
}

/// The complete build IR — all data from all source files.
pub(super) struct BuildIR {
    pub globals: Vec<IRStmt>,                   // VarDecl entries
    pub functions: HashMap<String, IRFunc>,
    pub bare_stmts: Vec<IRStmt>,
    /// Names of all `native` declarations across the import tree.
    /// Used by the AS build to prefix calls with `Jass::`.
    pub native_names: HashSet<String>,
}

// ─── Topological sort ────────────────────────────────────────────────────────

/// Topological sort of IR functions by callees using DFS.
pub(super) fn topo_sort_ir(functions: &HashMap<String, IRFunc>) -> Vec<String> {
    let mut visited = HashSet::new();
    let mut order = Vec::new();

    fn dfs(
        name: &str,
        functions: &HashMap<String, IRFunc>,
        visited: &mut HashSet<String>,
        order: &mut Vec<String>,
    ) {
        if visited.contains(name) { return; }
        visited.insert(name.to_string());
        if let Some(func) = functions.get(name) {
            for callee in &func.callees {
                if functions.contains_key(callee) {
                    dfs(callee, functions, visited, order);
                }
            }
        }
        order.push(name.to_string());
    }

    let mut names: Vec<&String> = functions.keys().collect();
    names.sort();
    for name in names {
        dfs(name, functions, &mut visited, &mut order);
    }

    // Enforce: config first, main last.
    let config_pos = order.iter().position(|n| n == "config");
    if let Some(pos) = config_pos {
        let config = order.remove(pos);
        order.insert(0, config);
    }
    let main_pos = order.iter().position(|n| n == "main");
    if let Some(pos) = main_pos {
        let main = order.remove(pos);
        order.push(main);
    }

    order
}

