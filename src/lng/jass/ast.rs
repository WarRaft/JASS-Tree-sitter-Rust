use crate::lng::jass::kind::{Field, Kind};
use tree_sitter::Node;

const FIELD_NAME: u16 = Field::Name as u16;
const FIELD_TYPE: u16 = Field::Type as u16;
const FIELD_BASE: u16 = Field::Base as u16;
const FIELD_RETURN_TYPE: u16 = Field::ReturnType as u16;
const FIELD_PARAMETERS: u16 = Field::Parameters as u16;
const FIELD_VALUE: u16 = Field::Value as u16;
const FIELD_VARIABLE: u16 = Field::Variable as u16;
const FIELD_INDEX: u16 = Field::Index as u16;
const FIELD_ARGS: u16 = Field::Args as u16;
const FIELD_CONDITION: u16 = Field::Condition as u16;

// ─── Semantic role for identifiers ───────────────────────────────────────────

/// Describes the semantic role an `id` plays in the AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdRole {
    /// Function/native declaration name.
    FunctionDecl,
    /// Type declaration name (in `type X extends Y` — the X).
    TypeDecl,
    /// Type reference (return type, parameter type, local type, extends base, var type).
    TypeRef,
    /// Parameter name.
    Param,
    /// Variable name (global var, local var, set target).
    Variable,
    /// Constant variable name.
    Constant,
    /// Function name in a call expression.
    FunctionRef,
}

// ─── AST nodes ───────────────────────────────────────────────────────────────

/// An identifier — CST node + semantic role.
#[derive(Debug, Clone)]
pub struct Id<'tree> {
    pub node: Node<'tree>,
    pub role: IdRole,
}

/// A CST error/missing node captured during AST build.
#[derive(Debug, Clone)]
pub struct CstError<'tree> {
    pub node: Node<'tree>,
    pub message: String,
}

/// `type <name> extends <base>`
#[derive(Debug, Clone)]
pub struct TypeDecl<'tree> {
    pub node: Node<'tree>,
    pub name: Option<Id<'tree>>,
    pub base: Option<Id<'tree>>,
}

/// `<type> <name>`
#[derive(Debug, Clone)]
pub struct Param<'tree> {
    pub node: Node<'tree>,
    pub type_id: Option<Id<'tree>>,
    pub name: Option<Id<'tree>>,
}

/// `[constant] native <name> takes <params> returns <return_type>`
#[derive(Debug, Clone)]
pub struct NativeDecl<'tree> {
    pub node: Node<'tree>,
    pub is_constant: bool,
    pub name: Option<Id<'tree>>,
    pub params: Vec<Param<'tree>>,
    pub return_type: Option<Id<'tree>>,
}

/// `[constant] function <name> takes <params> returns <return_type> ... endfunction`
#[derive(Debug, Clone)]
pub struct FunctionDecl<'tree> {
    pub node: Node<'tree>,
    pub is_constant: bool,
    pub name: Option<Id<'tree>>,
    pub params: Vec<Param<'tree>>,
    pub return_type: Option<Id<'tree>>,
    pub body: Vec<Statement<'tree>>,
}

/// A single variable inside `var_stmt`: `<name> [= <value>]`
#[derive(Debug, Clone)]
pub struct VarInit<'tree> {
    pub node: Node<'tree>,
    pub name: Option<Id<'tree>>,
    pub value: Option<Expr<'tree>>,
}

/// `[constant] <type> [array] <decls>`  (inside globals)
#[derive(Debug, Clone)]
pub struct VarStmt<'tree> {
    pub node: Node<'tree>,
    pub is_constant: bool,
    pub is_array: bool,
    pub type_id: Option<Id<'tree>>,
    pub decls: Vec<VarInit<'tree>>,
}

/// `local <type> [array] <name> [= <value>]`
#[derive(Debug, Clone)]
pub struct LocalDecl<'tree> {
    pub node: Node<'tree>,
    pub is_array: bool,
    pub type_id: Option<Id<'tree>>,
    pub name: Option<Id<'tree>>,
    pub value: Option<Expr<'tree>>,
}

/// `set <variable>[<index>] = <value>`
#[derive(Debug, Clone)]
pub struct SetStmt<'tree> {
    pub node: Node<'tree>,
    pub variable: Option<Id<'tree>>,
    pub index: Option<Expr<'tree>>,
    pub value: Option<Expr<'tree>>,
}

/// `call <function_call>`
#[derive(Debug, Clone)]
pub struct CallStmt<'tree> {
    pub node: Node<'tree>,
    pub func: Option<FunctionCall<'tree>>,
}

/// `<name>(<args>)`
#[derive(Debug, Clone)]
pub struct FunctionCall<'tree> {
    pub node: Node<'tree>,
    pub name: Option<Id<'tree>>,
    pub args: Vec<Expr<'tree>>,
}

/// `return [<expr>]`
#[derive(Debug, Clone)]
pub struct ReturnStmt<'tree> {
    pub node: Node<'tree>,
    pub value: Option<Expr<'tree>>,
}

/// `exitwhen <expr>`
#[derive(Debug, Clone)]
pub struct ExitwhenStmt<'tree> {
    pub node: Node<'tree>,
    pub condition: Option<Expr<'tree>>,
}

/// One branch of an `if`/`elseif`/`else` block.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ElseBranch<'tree> {
    pub node: Node<'tree>,
    /// `Some` for `if`/`elseif` branches, `None` for `else`.
    pub condition: Option<Expr<'tree>>,
    pub body: Vec<Statement<'tree>>,
}

/// `if <cond> then ... [elseif ...] [else ...] endif`
#[derive(Debug, Clone)]
pub struct IfStmt<'tree> {
    pub node: Node<'tree>,
    /// The first branch's condition (`if <cond>`).
    pub condition: Option<Expr<'tree>>,
    /// The first branch's body (statements between `then` and the next
    /// `elseif`/`else`/`endif`).
    pub body: Vec<Statement<'tree>>,
    /// `elseif` and `else` branches (in source order).
    pub branches: Vec<ElseBranch<'tree>>,
}

/// `loop ... endloop`
#[derive(Debug, Clone)]
pub struct LoopStmt<'tree> {
    pub node: Node<'tree>,
    pub body: Vec<Statement<'tree>>,
}

/// `globals ... endglobals`
#[derive(Debug, Clone)]
pub struct GlobalsBlock<'tree> {
    pub node: Node<'tree>,
    pub vars: Vec<VarStmt<'tree>>,
}

/// A comment line.
#[derive(Debug, Clone)]
pub struct Comment<'tree> {
    pub node: Node<'tree>,
}

// Re-export shared directive types for convenience.
pub use crate::lng::directive::{IgnoreDirective, ImportDirective, SetDirective, UjapiDirective};

// ─── Expressions ─────────────────────────────────────────────────────────────

/// Expression node in the AST.
#[derive(Debug, Clone)]
pub enum Expr<'tree> {
    /// Variable / identifier reference.
    Id(Id<'tree>),
    /// `function_call`: `name(args...)`
    Call(FunctionCall<'tree>),
    /// `function <name>` — function reference expression.
    FuncRef(Id<'tree>),
    /// Binary: `left OP right`
    Binary {
        node: Node<'tree>,
        left: Box<Expr<'tree>>,
        right: Box<Expr<'tree>>,
    },
    /// Unary: `not expr`, `-expr`
    Unary {
        node: Node<'tree>,
        operand: Box<Expr<'tree>>,
    },
    /// Parenthesized: `(expr)`
    Parens {
        node: Node<'tree>,
        inner: Box<Expr<'tree>>,
    },
    /// Array index: `expr[expr]`
    Index {
        node: Node<'tree>,
        array: Box<Expr<'tree>>,
        index: Box<Expr<'tree>>,
    },
    /// Literal (number, rawcode, string, etc.)
    Literal(Node<'tree>),
}

/// Any top-level or body statement.
#[derive(Debug, Clone)]
pub enum Statement<'tree> {
    Type(TypeDecl<'tree>),
    Native(NativeDecl<'tree>),
    Function(FunctionDecl<'tree>),
    Globals(GlobalsBlock<'tree>),
    Local(LocalDecl<'tree>),
    Set(SetStmt<'tree>),
    Call(CallStmt<'tree>),
    Return(ReturnStmt<'tree>),
    Exitwhen(ExitwhenStmt<'tree>),
    If(IfStmt<'tree>),
    Loop(LoopStmt<'tree>),
    VarStmt(VarStmt<'tree>),
    Comment(Comment<'tree>),
    Import(ImportDirective<'tree>),
    SetDir(SetDirective<'tree>),
    IgnoreDir(IgnoreDirective<'tree>),
    UjapiImport(UjapiDirective<'tree>),
    /// Unrecognized or error CST node preserved in the AST so it participates
    /// in ordering (e.g. blocks import scanning) and can be highlighted.
    Error(CstError<'tree>),
}

/// The root of the AST.
#[derive(Debug, Clone)]
pub struct Ast<'tree> {
    pub items: Vec<Statement<'tree>>,
    pub errors: Vec<CstError<'tree>>,
}

// ─── Building the AST from CST ──────────────────────────────────────────────

/// Build the AST from a tree-sitter CST root node.
///
/// Root-level `//import` / `//import!` comments are **not** rewritten here —
/// call [`rewrite_imports`] afterwards with the source bytes.
pub fn build_ast<'tree>(root: Node<'tree>) -> Ast<'tree> {
    let mut errors = Vec::new();
    let items = build_children(&root, &mut errors, true);

    Ast { items, errors }
}

/// Rewrite leading root-level comments into `Statement::Import` or
/// `Statement::SetDir` when they match the `//import` / `//import!` /
/// `//set` patterns.
///
/// Only comments **before the first non-comment statement** are considered.
/// `Statement::Error` nodes (e.g. `a = 2`) also count as "real code" and
/// stop the scan — later directives stay as plain comments.
///
/// Must be called after `build_ast` and **before** any cursor / parse logic
/// that depends on the distinction.
///
/// `src` — full file source (UTF-8 bytes).
pub fn rewrite_imports(ast: &mut Ast, src: &[u8]) {
    use crate::lng::directive::{try_parse_directive, Directive};

    let mut i = 0;
    while i < ast.items.len() {
        // Stop at first non-comment, non-directive item.
        match &ast.items[i] {
            Statement::Comment(_) => {}
            Statement::Import(_) | Statement::SetDir(_) | Statement::IgnoreDir(_) | Statement::UjapiImport(_) => {
                i += 1;
                continue;
            }
            _ => break,
        }

        if let Statement::Comment(c) = &ast.items[i] {
            if let Some(dir) = try_parse_directive(&c.node, src) {
                ast.items[i] = match dir {
                    Directive::Import(imp) => Statement::Import(imp),
                    Directive::Set(sd) => Statement::SetDir(sd),
                    Directive::Ignore(ig) => Statement::IgnoreDir(ig),
                    Directive::Ujapi(ud) => Statement::UjapiImport(ud),
                };
                i += 1;
                continue;
            }
        }
        i += 1;
    }
}

fn collect_errors<'tree>(node: &Node<'tree>, errors: &mut Vec<CstError<'tree>>) {
    if node.is_missing() {
        errors.push(CstError {
            node: *node,
            message: crate::util::i18n::missing_token(node.kind()),
        });
    } else if node.is_error() {
        errors.push(CstError {
            node: *node,
            message: crate::util::i18n::syntax_error().into(),
        });
    }
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            collect_errors(&child, errors);
        }
    }
}

fn build_children<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
    capture_unknown: bool,
) -> Vec<Statement<'tree>> {
    let mut stmts = Vec::new();
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if child.is_error() || child.is_missing() {
                collect_errors(&child, errors);
            }
            if let Some(stmt) = build_statement(&child, errors) {
                stmts.push(stmt);
            } else if capture_unknown && (child.is_error() || child.is_named()) {
                // Any named CST node that didn't map to a known statement
                // (including ERROR nodes and unexpected constructs like bare
                // expressions) is preserved so it blocks import scanning
            } else if capture_unknown && (child.is_error() || child.is_named()) {
                stmts.push(Statement::Error(CstError {
                    node: child,
                    message: if child.is_error() {
                        crate::util::i18n::syntax_error().into()
                    } else {
                        crate::util::i18n::unexpected_node(child.kind())
                    },
                }));
            }
        }
    }
    stmts
}

fn build_statement<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> Option<Statement<'tree>> {
    match Kind::try_from(node.kind_id()) {
        Ok(Kind::TypeStatement) => Some(Statement::Type(build_type_decl(node))),
        Ok(Kind::NativeStatement) => Some(Statement::Native(build_native_decl(node))),
        Ok(Kind::FunctionStatement) => Some(Statement::Function(build_function_decl(node, errors))),
        Ok(Kind::GlobalsBlock) => Some(Statement::Globals(build_globals_block(node, errors))),
        Ok(Kind::LocalStatement) => Some(Statement::Local(build_local_decl(node))),
        Ok(Kind::SetStatement) => Some(Statement::Set(build_set_stmt(node))),
        Ok(Kind::CallStatement) => Some(Statement::Call(build_call_stmt(node))),
        Ok(Kind::ReturnStatement) => Some(Statement::Return(build_return_stmt(node))),
        Ok(Kind::ExitwhenStatement) => Some(Statement::Exitwhen(build_exitwhen_stmt(node))),
        Ok(Kind::IfStatement) => Some(Statement::If(build_if_stmt(node, errors))),
        Ok(Kind::LoopStatement) => Some(Statement::Loop(build_loop_stmt(node, errors))),
        Ok(Kind::VarStmt) => Some(Statement::VarStmt(build_var_stmt(node))),
        Ok(Kind::Comment) => Some(Statement::Comment(Comment { node: *node })),
        // Bare expression at statement level — e.g. `A = 21` without `set` keyword.
        // The grammar parses this as `expr { expr(id) "=" expr(value) }`.
        // We convert assignment expressions to Statement::Set.
        Ok(Kind::Expr) => build_expr_statement(node),
        _ => None,
    }
}

/// Convert a bare `expr` at statement level.
/// If it's an assignment (`id = value`), produce `Statement::Set`.
/// Otherwise, wrap the whole thing as `Statement::Set` with only the expression.
fn build_expr_statement<'tree>(node: &Node<'tree>) -> Option<Statement<'tree>> {
    // Look for assignment pattern: child0=expr(id), child1='=', child2=expr(value)
    let count = node.child_count();
    if count >= 3 {
        // Check if there's a '=' token among children
        for i in 0..count {
            if let Some(eq) = node.child(i as u32) {
                if Kind::try_from(eq.kind_id()) == Ok(Kind::Equal) {
                    // Left side = everything before '='
                    let left = if i > 0 { node.child(0) } else { None };
                    // Right side = everything after '='
                    let right = if (i as u32 + 1) < count as u32 {
                        node.child(i as u32 + 1)
                    } else {
                        None
                    };

                    // Extract variable id from left expr
                    let variable = left.and_then(|l| {
                        // The left side is an expr wrapping an id
                        find_id_in_expr(&l)
                    });

                    // Check if left side is an array access: id[index]
                    let index = left.and_then(|l| find_index_in_expr(&l));

                    let value = right.and_then(|r| build_expr(&r));

                    return Some(Statement::Set(SetStmt {
                        node: *node,
                        variable,
                        index,
                        value,
                    }));
                }
            }
        }
    }
    // Not an assignment — still process as expression for ref tracking
    None
}

/// Find an `id` node inside an expr (possibly nested one level).
fn find_id_in_expr<'tree>(node: &Node<'tree>) -> Option<Id<'tree>> {
    if Kind::try_from(node.kind_id()) == Ok(Kind::Id) {
        return Some(build_id(node, IdRole::Variable));
    }
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if Kind::try_from(child.kind_id()) == Ok(Kind::Id) {
                return Some(build_id(&child, IdRole::Variable));
            }
        }
    }
    None
}

/// Find an index expression in an array access expr (e.g. `A[i]`).
fn find_index_in_expr<'tree>(node: &Node<'tree>) -> Option<Expr<'tree>> {
    // Look for pattern: expr { id "[" expr "]" }
    let count = node.child_count();
    let mut found_bracket = false;
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if Kind::try_from(child.kind_id()) == Ok(Kind::LeftBracket) {
                found_bracket = true;
            } else if found_bracket && Kind::try_from(child.kind_id()) != Ok(Kind::RightBracket) {
                return build_expr(&child);
            }
        }
    }
    None
}

fn build_id<'tree>(node: &Node<'tree>, role: IdRole) -> Id<'tree> {
    Id { node: *node, role }
}

fn maybe_id<'tree>(node: &Node<'tree>, field: u16, role: IdRole) -> Option<Id<'tree>> {
    node.child_by_field_id(field).and_then(|n| {
        if Kind::try_from(n.kind_id()) == Ok(Kind::Id) {
            Some(build_id(&n, role))
        } else {
            None
        }
    })
}

fn build_type_decl<'tree>(node: &Node<'tree>) -> TypeDecl<'tree> {
    TypeDecl {
        node: *node,
        name: maybe_id(node, FIELD_NAME, IdRole::TypeDecl),
        base: maybe_id(node, FIELD_BASE, IdRole::TypeRef),
    }
}

fn build_params<'tree>(node: &Node<'tree>) -> Vec<Param<'tree>> {
    let mut params = Vec::new();
    if let Some(pl) = node.child_by_field_id(FIELD_PARAMETERS) {
        if Kind::try_from(pl.kind_id()) == Ok(Kind::ParameterList) {
            let count = pl.child_count();
            for i in 0..count {
                if let Some(child) = pl.child(i as u32) {
                    if Kind::try_from(child.kind_id()) == Ok(Kind::Parameter) {
                        params.push(Param {
                            node: child,
                            type_id: maybe_id(&child, FIELD_TYPE, IdRole::TypeRef),
                            name: maybe_id(&child, FIELD_NAME, IdRole::Param),
                        });
                    }
                }
            }
        }
    }
    params
}

fn build_native_decl<'tree>(node: &Node<'tree>) -> NativeDecl<'tree> {
    NativeDecl {
        node: *node,
        is_constant: has_keyword(node, Kind::Constant),
        name: maybe_id(node, FIELD_NAME, IdRole::FunctionDecl),
        params: build_params(node),
        return_type: maybe_id(node, FIELD_RETURN_TYPE, IdRole::TypeRef),
    }
}

fn build_function_decl<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> FunctionDecl<'tree> {
    FunctionDecl {
        node: *node,
        is_constant: has_keyword(node, Kind::Constant),
        name: maybe_id(node, FIELD_NAME, IdRole::FunctionDecl),
        params: build_params(node),
        return_type: maybe_id(node, FIELD_RETURN_TYPE, IdRole::TypeRef),
        body: build_children(node, errors, false),
    }
}

fn build_globals_block<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> GlobalsBlock<'tree> {
    let mut vars = Vec::new();
    for stmt in build_children(node, errors, false) {
        if let Statement::VarStmt(v) = stmt {
            vars.push(v);
        }
    }
    GlobalsBlock {
        node: *node,
        vars,
    }
}

fn has_keyword(node: &Node, kw: Kind) -> bool {
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if Kind::try_from(child.grammar_id()) == Ok(kw) {
                return true;
            }
        }
    }
    false
}

fn build_var_stmt<'tree>(node: &Node<'tree>) -> VarStmt<'tree> {
    let is_constant = has_keyword(node, Kind::Constant);
    let is_array = has_keyword(node, Kind::Array);
    let type_id = maybe_id(node, FIELD_TYPE, IdRole::TypeRef);
    let var_role = if is_constant { IdRole::Constant } else { IdRole::Variable };

    let mut decls = Vec::new();
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if Kind::try_from(child.kind_id()) == Ok(Kind::VarDecl) {
                decls.push(VarInit {
                    node: child,
                    name: maybe_id(&child, FIELD_NAME, var_role),
                    value: child.child_by_field_id(FIELD_VALUE).and_then(|n| build_expr(&n)),
                });
            }
        }
    }

    VarStmt {
        node: *node,
        is_constant,
        is_array,
        type_id,
        decls,
    }
}

fn build_local_decl<'tree>(node: &Node<'tree>) -> LocalDecl<'tree> {
    LocalDecl {
        node: *node,
        is_array: has_keyword(node, Kind::Array),
        type_id: maybe_id(node, FIELD_TYPE, IdRole::TypeRef),
        name: maybe_id(node, FIELD_NAME, IdRole::Variable),
        value: node.child_by_field_id(FIELD_VALUE).and_then(|n| build_expr(&n)),
    }
}

fn build_set_stmt<'tree>(node: &Node<'tree>) -> SetStmt<'tree> {
    SetStmt {
        node: *node,
        variable: maybe_id(node, FIELD_VARIABLE, IdRole::Variable),
        index: node.child_by_field_id(FIELD_INDEX).and_then(|n| build_expr(&n)),
        value: node.child_by_field_id(FIELD_VALUE).and_then(|n| build_expr(&n)),
    }
}

fn extract_call_name<'tree>(fc_node: &Node<'tree>) -> Option<Id<'tree>> {
    let name_expr = fc_node.child_by_field_id(FIELD_NAME)?;
    let count = name_expr.child_count();
    for i in 0..count {
        if let Some(child) = name_expr.child(i as u32) {
            if Kind::try_from(child.kind_id()) == Ok(Kind::Id) {
                return Some(build_id(&child, IdRole::FunctionRef));
            }
        }
    }
    None
}

fn build_function_call<'tree>(node: &Node<'tree>) -> FunctionCall<'tree> {
    let mut args = Vec::new();
    if let Some(args_node) = node.child_by_field_id(FIELD_ARGS) {
        let count = args_node.child_count();
        for i in 0..count {
            if let Some(child) = args_node.child(i as u32) {
                if Kind::try_from(child.kind_id()) == Ok(Kind::Expr) {
                    if let Some(expr) = build_expr(&child) {
                        args.push(expr);
                    }
                }
            }
        }
    }
    FunctionCall {
        node: *node,
        name: extract_call_name(node),
        args,
    }
}

fn build_call_stmt<'tree>(node: &Node<'tree>) -> CallStmt<'tree> {
    let mut func = None;
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if Kind::try_from(child.kind_id()) == Ok(Kind::FunctionCall) {
                func = Some(build_function_call(&child));
                break;
            }
        }
    }
    CallStmt { node: *node, func }
}

fn build_return_stmt<'tree>(node: &Node<'tree>) -> ReturnStmt<'tree> {
    let mut value = None;
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if Kind::try_from(child.kind_id()) == Ok(Kind::Expr) {
                value = build_expr(&child);
                break;
            }
        }
    }
    ReturnStmt { node: *node, value }
}

fn build_exitwhen_stmt<'tree>(node: &Node<'tree>) -> ExitwhenStmt<'tree> {
    let mut condition = None;
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if Kind::try_from(child.kind_id()) == Ok(Kind::Expr) {
                condition = build_expr(&child);
                break;
            }
        }
    }
    ExitwhenStmt { node: *node, condition }
}

fn build_if_stmt<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> IfStmt<'tree> {
    let condition = node.child_by_field_id(FIELD_CONDITION).and_then(|n| build_expr(&n));

    // Walk CST children to split statements into branches.
    //
    // CST structure:
    //   if COND then STMTS [elseif COND then STMTS]* [else STMTS] endif
    //
    // We collect the first branch's body separately, and subsequent
    // elseif/else branches into `branches`.
    let mut first_body: Vec<Statement<'tree>> = Vec::new();
    let mut branches: Vec<ElseBranch<'tree>> = Vec::new();

    // State machine: which section are we collecting into?
    enum Phase { SkipToThen, FirstBody, ElseifCond, ElseifBody, ElseBody }
    let mut phase = Phase::SkipToThen;
    let mut pending_cond: Option<Expr<'tree>> = None;
    let mut pending_stmts: Vec<Statement<'tree>> = Vec::new();
    let mut branch_node: Option<Node<'tree>> = None;

    let count = node.child_count();
    for i in 0..count {
        let child = match node.child(i as u32) {
            Some(c) => c,
            None => continue,
        };

        if child.is_error() || child.is_missing() {
            collect_errors(&child, errors);
        }

        let kind = Kind::try_from(child.grammar_id()).ok();
        match (&phase, kind) {
            // Skip past `if` and condition until we see `then`.
            (Phase::SkipToThen, Some(Kind::Then)) => {
                phase = Phase::FirstBody;
            }
            (Phase::SkipToThen, _) => { /* skip if keyword + condition */ }

            // First branch body — collect statements until elseif/else/endif.
            (Phase::FirstBody, Some(Kind::Elseif)) => {
                branch_node = Some(child);
                pending_cond = None;
                pending_stmts = Vec::new();
                phase = Phase::ElseifCond;
            }
            (Phase::FirstBody, Some(Kind::Else)) => {
                branch_node = Some(child);
                pending_stmts = Vec::new();
                phase = Phase::ElseBody;
            }
            (Phase::FirstBody, Some(Kind::Endif)) => { /* done */ }
            (Phase::FirstBody, _) => {
                if let Some(stmt) = build_statement(&child, errors) {
                    first_body.push(stmt);
                }
            }

            // Elseif condition — collect the condition expression.
            (Phase::ElseifCond, Some(Kind::Then)) => {
                phase = Phase::ElseifBody;
            }
            (Phase::ElseifCond, _) => {
                if child.is_named() && pending_cond.is_none() {
                    pending_cond = build_expr(&child);
                }
            }

            // Elseif body — collect statements.
            (Phase::ElseifBody, Some(Kind::Elseif)) => {
                // Flush current elseif branch.
                branches.push(ElseBranch {
                    node: branch_node.unwrap_or(child),
                    condition: pending_cond.take(),
                    body: std::mem::take(&mut pending_stmts),
                });
                branch_node = Some(child);
                phase = Phase::ElseifCond;
            }
            (Phase::ElseifBody, Some(Kind::Else)) => {
                // Flush current elseif branch.
                branches.push(ElseBranch {
                    node: branch_node.unwrap_or(child),
                    condition: pending_cond.take(),
                    body: std::mem::take(&mut pending_stmts),
                });
                branch_node = Some(child);
                phase = Phase::ElseBody;
            }
            (Phase::ElseifBody, Some(Kind::Endif)) => {
                // Flush last elseif branch.
                branches.push(ElseBranch {
                    node: branch_node.unwrap_or(child),
                    condition: pending_cond.take(),
                    body: std::mem::take(&mut pending_stmts),
                });
            }
            (Phase::ElseifBody, _) => {
                if let Some(stmt) = build_statement(&child, errors) {
                    pending_stmts.push(stmt);
                }
            }

            // Else body — collect statements.
            (Phase::ElseBody, Some(Kind::Endif)) => {
                branches.push(ElseBranch {
                    node: branch_node.unwrap_or(child),
                    condition: None, // else has no condition
                    body: std::mem::take(&mut pending_stmts),
                });
            }
            (Phase::ElseBody, _) => {
                if let Some(stmt) = build_statement(&child, errors) {
                    pending_stmts.push(stmt);
                }
            }
        }
    }

    IfStmt {
        node: *node,
        condition,
        body: first_body,
        branches,
    }
}

fn build_loop_stmt<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> LoopStmt<'tree> {
    LoopStmt {
        node: *node,
        body: build_children(node, errors, false),
    }
}

// ─── Expression builder ─────────────────────────────────────────────────────

pub(crate) fn build_expr<'tree>(node: &Node<'tree>) -> Option<Expr<'tree>> {
    let kind = Kind::try_from(node.kind_id()).ok()?;
    match kind {
        Kind::Expr => build_expr_inner(node),
        Kind::FunctionCall => Some(Expr::Call(build_function_call(node))),
        Kind::FunctionRef => {
            let name = maybe_id(node, FIELD_NAME, IdRole::FunctionRef)?;
            Some(Expr::FuncRef(name))
        }
        Kind::Id => Some(Expr::Id(build_id(node, IdRole::Variable))),
        Kind::Parens => {
            let inner = find_child_expr(node)?;
            Some(Expr::Parens {
                node: *node,
                inner: Box::new(inner),
            })
        }
        Kind::Number | Kind::Float | Kind::Rawcode | Kind::StringLiteral => {
            Some(Expr::Literal(*node))
        }
        _ => None,
    }
}

fn build_expr_inner<'tree>(node: &Node<'tree>) -> Option<Expr<'tree>> {
    let count = node.child_count();
    if count == 0 {
        return None;
    }

    if count == 1 {
        return node.child(0).and_then(|c| build_expr(&c));
    }

    // Array index: has "[" child
    let has_bracket = (0..count).any(|i| {
        node.child(i as u32)
            .map(|c| Kind::try_from(c.grammar_id()) == Ok(Kind::LeftBracket))
            .unwrap_or(false)
    });
    if has_bracket {
        let mut exprs = Vec::new();
        for i in 0..count {
            if let Some(child) = node.child(i as u32) {
                if Kind::try_from(child.kind_id()) == Ok(Kind::Expr) {
                    if let Some(e) = build_expr(&child) {
                        exprs.push(e);
                    }
                }
            }
        }
        if exprs.len() == 2 {
            let index = exprs.pop().unwrap();
            let array = exprs.pop().unwrap();
            return Some(Expr::Index {
                node: *node,
                array: Box::new(array),
                index: Box::new(index),
            });
        }
    }

    // Unary: first child is operator (not, -)
    if let Some(first) = node.child(0) {
        let first_kind = Kind::try_from(first.grammar_id()).ok();
        if first_kind == Some(Kind::Not) || first_kind == Some(Kind::Minus) {
            if let Some(operand) = find_child_expr(node) {
                return Some(Expr::Unary {
                    node: *node,
                    operand: Box::new(operand),
                });
            }
        }
    }

    // Binary: expr OP expr
    let mut expr_children = Vec::new();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if Kind::try_from(child.kind_id()) == Ok(Kind::Expr) {
                if let Some(e) = build_expr(&child) {
                    expr_children.push(e);
                }
            }
        }
    }
    if expr_children.len() == 2 {
        let right = expr_children.pop().unwrap();
        let left = expr_children.pop().unwrap();
        return Some(Expr::Binary {
            node: *node,
            left: Box::new(left),
            right: Box::new(right),
        });
    }

    // Single child
    if expr_children.len() == 1 {
        return expr_children.pop();
    }

    // Fallback
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if let Some(e) = build_expr(&child) {
                return Some(e);
            }
        }
    }
    None
}

fn find_child_expr<'tree>(node: &Node<'tree>) -> Option<Expr<'tree>> {
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if Kind::try_from(child.kind_id()) == Ok(Kind::Expr) {
                return build_expr(&child);
            }
        }
    }
    None
}


