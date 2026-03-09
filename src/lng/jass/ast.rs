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

/// `native <name> takes <params> returns <return_type>`
#[derive(Debug, Clone)]
pub struct NativeDecl<'tree> {
    pub node: Node<'tree>,
    pub name: Option<Id<'tree>>,
    pub params: Vec<Param<'tree>>,
    pub return_type: Option<Id<'tree>>,
}

/// `function <name> takes <params> returns <return_type> ... endfunction`
#[derive(Debug, Clone)]
pub struct FunctionDecl<'tree> {
    pub node: Node<'tree>,
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
    pub has_value: bool,
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
    pub type_id: Option<Id<'tree>>,
    pub name: Option<Id<'tree>>,
    pub has_value: bool,
}

/// `set <variable>[<index>] = <value>`
#[derive(Debug, Clone)]
pub struct SetStmt<'tree> {
    pub node: Node<'tree>,
    pub variable: Option<Id<'tree>>,
    pub has_index: bool,
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
    pub arg_count: usize,
}

/// `return [<expr>]`
#[derive(Debug, Clone)]
pub struct ReturnStmt<'tree> {
    pub node: Node<'tree>,
}

/// `exitwhen <expr>`
#[derive(Debug, Clone)]
pub struct ExitwhenStmt<'tree> {
    pub node: Node<'tree>,
}

/// `if <cond> then ... [elseif ...] [else ...] endif`
#[derive(Debug, Clone)]
pub struct IfStmt<'tree> {
    pub node: Node<'tree>,
    pub body: Vec<Statement<'tree>>,
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
}

/// The root of the AST.
#[derive(Debug, Clone)]
pub struct Ast<'tree> {
    pub items: Vec<Statement<'tree>>,
    pub errors: Vec<CstError<'tree>>,
}

// ─── Building the AST from CST ──────────────────────────────────────────────

/// Build the AST from a tree-sitter CST root node.
pub fn build_ast<'tree>(root: Node<'tree>) -> Ast<'tree> {
    let mut errors = Vec::new();
    let items = build_children(&root, &mut errors);
    Ast { items, errors }
}

fn collect_errors<'tree>(node: &Node<'tree>, errors: &mut Vec<CstError<'tree>>) {
    if node.is_missing() {
        errors.push(CstError {
            node: *node,
            message: format!("Missing `{}`", node.kind()),
        });
    } else if node.is_error() {
        errors.push(CstError {
            node: *node,
            message: "Syntax error".into(),
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
        Ok(Kind::ReturnStatement) => Some(Statement::Return(ReturnStmt { node: *node })),
        Ok(Kind::ExitwhenStatement) => Some(Statement::Exitwhen(ExitwhenStmt { node: *node })),
        Ok(Kind::IfStatement) => Some(Statement::If(build_if_stmt(node, errors))),
        Ok(Kind::LoopStatement) => Some(Statement::Loop(build_loop_stmt(node, errors))),
        Ok(Kind::VarStmt) => Some(Statement::VarStmt(build_var_stmt(node))),
        Ok(Kind::Comment) => Some(Statement::Comment(Comment { node: *node })),
        _ => None,
    }
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
        name: maybe_id(node, FIELD_NAME, IdRole::FunctionDecl),
        params: build_params(node),
        return_type: maybe_id(node, FIELD_RETURN_TYPE, IdRole::TypeRef),
        body: build_children(node, errors),
    }
}

fn build_globals_block<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> GlobalsBlock<'tree> {
    let mut vars = Vec::new();
    for stmt in build_children(node, errors) {
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
                    has_value: child.child_by_field_id(FIELD_VALUE).is_some(),
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
        type_id: maybe_id(node, FIELD_TYPE, IdRole::TypeRef),
        name: maybe_id(node, FIELD_NAME, IdRole::Variable),
        has_value: node.child_by_field_id(FIELD_VALUE).is_some(),
    }
}

fn build_set_stmt<'tree>(node: &Node<'tree>) -> SetStmt<'tree> {
    SetStmt {
        node: *node,
        variable: maybe_id(node, FIELD_VARIABLE, IdRole::Variable),
        has_index: node.child_by_field_id(FIELD_INDEX).is_some(),
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
    let arg_count = node
        .child_by_field_id(FIELD_ARGS)
        .map(|args| {
            let mut count = 0usize;
            let total = args.child_count();
            for i in 0..total {
                if let Some(child) = args.child(i as u32) {
                    if Kind::try_from(child.kind_id()) == Ok(Kind::Expr) {
                        count += 1;
                    }
                }
            }
            count
        })
        .unwrap_or(0);
    FunctionCall {
        node: *node,
        name: extract_call_name(node),
        arg_count,
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

fn build_if_stmt<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> IfStmt<'tree> {
    IfStmt {
        node: *node,
        body: build_children(node, errors),
    }
}

fn build_loop_stmt<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> LoopStmt<'tree> {
    LoopStmt {
        node: *node,
        body: build_children(node, errors),
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse JASS source, keeping tree alive, and run assertion closure on AST.
    fn with_ast(src: &str, f: impl FnOnce(&Ast)) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        f(&ast);
    }

    fn node_text<'a>(src: &'a str, node: &Node) -> &'a str {
        &src[node.start_byte()..node.end_byte()]
    }

    #[test]
    fn type_statement() {
        let src = "type handle extends agent\n";
        with_ast(src, |ast| {
            assert_eq!(ast.items.len(), 1);
            assert!(ast.errors.is_empty());
            match &ast.items[0] {
                Statement::Type(t) => {
                    let name = t.name.as_ref().unwrap();
                    assert_eq!(node_text(src, &name.node), "handle");
                    assert_eq!(name.role, IdRole::TypeDecl);
                    let base = t.base.as_ref().unwrap();
                    assert_eq!(node_text(src, &base.node), "agent");
                    assert_eq!(base.role, IdRole::TypeRef);
                }
                other => panic!("Expected Type, got {:?}", other),
            }
        });
    }

    #[test]
    fn native_takes_nothing() {
        let src = "native Foo takes nothing returns nothing\n";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Native(n) => {
                    assert_eq!(node_text(src, &n.name.as_ref().unwrap().node), "Foo");
                    assert_eq!(n.name.as_ref().unwrap().role, IdRole::FunctionDecl);
                    assert!(n.params.is_empty());
                    assert!(n.return_type.is_none());
                }
                other => panic!("Expected Native, got {:?}", other),
            }
        });
    }

    #[test]
    fn call_statement_function_ref() {
        let src = "call Foo(a, b, c)\n";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Call(c) => {
                    let fc = c.func.as_ref().unwrap();
                    assert_eq!(node_text(src, &fc.name.as_ref().unwrap().node), "Foo");
                    assert_eq!(fc.name.as_ref().unwrap().role, IdRole::FunctionRef);
                    assert_eq!(fc.arg_count, 3);
                }
                other => panic!("Expected Call, got {:?}", other),
            }
        });
    }

    #[test]
    fn function_body() {
        let src = "\
function F takes integer x returns nothing
    local integer y = 1
    set y = 2
    return
endfunction
";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Function(f) => {
                    assert_eq!(f.params.len(), 1);
                    assert_eq!(f.body.len(), 3);
                    match &f.body[0] {
                        Statement::Local(l) => {
                            assert_eq!(l.name.as_ref().unwrap().role, IdRole::Variable);
                            assert!(l.has_value);
                        }
                        other => panic!("Expected Local, got {:?}", other),
                    }
                    match &f.body[1] {
                        Statement::Set(s) => assert_eq!(s.variable.as_ref().unwrap().role, IdRole::Variable),
                        other => panic!("Expected Set, got {:?}", other),
                    }
                }
                other => panic!("Expected Function, got {:?}", other),
            }
        });
    }

    #[test]
    fn globals_constant() {
        let src = "\
globals
    constant integer MAX = 100
    real x
endglobals
";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Globals(g) => {
                    assert_eq!(g.vars.len(), 2);
                    assert!(g.vars[0].is_constant);
                    assert_eq!(g.vars[0].decls[0].name.as_ref().unwrap().role, IdRole::Constant);
                    assert!(!g.vars[1].is_constant);
                    assert_eq!(g.vars[1].decls[0].name.as_ref().unwrap().role, IdRole::Variable);
                }
                other => panic!("Expected Globals, got {:?}", other),
            }
        });
    }

    #[test]
    fn collects_errors() {
        let src = "function\n";
        with_ast(src, |ast| {
            assert!(!ast.errors.is_empty());
        });
    }

    #[test]
    fn node_positions_preserved() {
        let src = "type handle extends agent\n";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Type(t) => {
                    assert_eq!(t.node.start_byte(), 0);
                    assert_eq!(t.node.end_byte(), 25);
                    let name = t.name.as_ref().unwrap();
                    assert_eq!(name.node.start_byte(), 5);
                    assert_eq!(name.node.end_byte(), 11);
                }
                _ => panic!("Expected Type"),
            }
        });
    }
}

