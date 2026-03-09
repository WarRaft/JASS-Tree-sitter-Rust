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

/// `if <cond> then ... [elseif ...] [else ...] endif`
#[derive(Debug, Clone)]
pub struct IfStmt<'tree> {
    pub node: Node<'tree>,
    pub condition: Option<Expr<'tree>>,
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
        Ok(Kind::ReturnStatement) => Some(Statement::Return(build_return_stmt(node))),
        Ok(Kind::ExitwhenStatement) => Some(Statement::Exitwhen(build_exitwhen_stmt(node))),
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
    IfStmt {
        node: *node,
        condition: node.child_by_field_id(FIELD_CONDITION).and_then(|n| build_expr(&n)),
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

// ─── Expression builder ─────────────────────────────────────────────────────

fn build_expr<'tree>(node: &Node<'tree>) -> Option<Expr<'tree>> {
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

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
    fn call_args() {
        let src = "call Foo(a, b, c)\n";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Call(c) => {
                    let fc = c.func.as_ref().unwrap();
                    assert_eq!(fc.name.as_ref().unwrap().role, IdRole::FunctionRef);
                    assert_eq!(fc.args.len(), 3);
                    match &fc.args[0] {
                        Expr::Id(id) => assert_eq!(node_text(src, &id.node), "a"),
                        other => panic!("Expected Id, got {:?}", other),
                    }
                }
                other => panic!("Expected Call, got {:?}", other),
            }
        });
    }

    #[test]
    fn local_value_expr() {
        let src = "local integer a = x + 1\n";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Local(l) => {
                    assert!(l.value.is_some());
                    match l.value.as_ref().unwrap() {
                        Expr::Binary { left, right, .. } => {
                            assert!(matches!(left.as_ref(), Expr::Id(_)));
                            assert!(matches!(right.as_ref(), Expr::Literal(_)));
                        }
                        other => panic!("Expected Binary, got {:?}", other),
                    }
                }
                other => panic!("Expected Local, got {:?}", other),
            }
        });
    }

    #[test]
    fn set_index_and_value() {
        let src = "set arr[i] = 5\n";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Set(s) => {
                    assert!(s.index.is_some());
                    match s.index.as_ref().unwrap() {
                        Expr::Id(id) => assert_eq!(node_text(src, &id.node), "i"),
                        other => panic!("Expected Id index, got {:?}", other),
                    }
                    assert!(s.value.is_some());
                }
                other => panic!("Expected Set, got {:?}", other),
            }
        });
    }

    #[test]
    fn return_function_call_expr() {
        let src = "\
function F takes unit t returns boolean
    return UnitLife(t) > 0
endfunction
";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Function(f) => {
                    match &f.body[0] {
                        Statement::Return(r) => {
                            match r.value.as_ref().unwrap() {
                                Expr::Binary { left, .. } => {
                                    match left.as_ref() {
                                        Expr::Call(fc) => {
                                            let name = fc.name.as_ref().unwrap();
                                            assert_eq!(node_text(src, &name.node), "UnitLife");
                                            assert_eq!(name.role, IdRole::FunctionRef);
                                            assert_eq!(fc.args.len(), 1);
                                        }
                                        other => panic!("Expected Call, got {:?}", other),
                                    }
                                }
                                other => panic!("Expected Binary, got {:?}", other),
                            }
                        }
                        other => panic!("Expected Return, got {:?}", other),
                    }
                }
                other => panic!("Expected Function, got {:?}", other),
            }
        });
    }

    #[test]
    fn exitwhen_condition() {
        let src = "exitwhen not b\n";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Exitwhen(e) => {
                    match e.condition.as_ref().unwrap() {
                        Expr::Unary { operand, .. } => {
                            assert!(matches!(operand.as_ref(), Expr::Id(_)));
                        }
                        other => panic!("Expected Unary, got {:?}", other),
                    }
                }
                other => panic!("Expected Exitwhen, got {:?}", other),
            }
        });
    }

    #[test]
    fn if_condition() {
        let src = "\
function F takes nothing returns nothing
    if a > 0 then
        return
    endif
endfunction
";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Function(f) => match &f.body[0] {
                    Statement::If(i) => assert!(matches!(i.condition.as_ref().unwrap(), Expr::Binary { .. })),
                    other => panic!("Expected If, got {:?}", other),
                },
                other => panic!("Expected Function, got {:?}", other),
            }
        });
    }

    #[test]
    fn function_ref_expr() {
        let src = "\
function F takes nothing returns nothing
    return function G
endfunction
";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Function(f) => match &f.body[0] {
                    Statement::Return(r) => match r.value.as_ref().unwrap() {
                        Expr::FuncRef(id) => {
                            assert_eq!(node_text(src, &id.node), "G");
                            assert_eq!(id.role, IdRole::FunctionRef);
                        }
                        other => panic!("Expected FuncRef, got {:?}", other),
                    },
                    other => panic!("Expected Return, got {:?}", other),
                },
                other => panic!("Expected Function, got {:?}", other),
            }
        });
    }

    #[test]
    fn parens_and_index_in_set() {
        let src = "set a = (a * 2) - arr[x]\n";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Set(s) => {
                    match s.value.as_ref().unwrap() {
                        Expr::Binary { left, right, .. } => {
                            assert!(matches!(left.as_ref(), Expr::Parens { .. }));
                            assert!(matches!(right.as_ref(), Expr::Index { .. }));
                        }
                        other => panic!("Expected Binary, got {:?}", other),
                    }
                }
                other => panic!("Expected Set, got {:?}", other),
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
}

