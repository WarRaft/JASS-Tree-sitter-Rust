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

// ─── Span ────────────────────────────────────────────────────────────────────

/// Byte range + row range in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_row: usize,
    pub end_row: usize,
}

impl Span {
    pub fn from_node(node: &Node) -> Self {
        Self {
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            start_row: node.start_position().row,
            end_row: node.end_position().row,
        }
    }
}

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

/// An identifier — wraps the byte span and its semantic role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Id {
    pub span: Span,
    pub role: IdRole,
}

/// A CST error/missing node captured during AST build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CstError {
    pub span: Span,
    pub message: String,
}

/// `type <name> extends <base>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDecl {
    pub span: Span,
    pub name: Option<Id>,
    pub base: Option<Id>,
}

/// `<type> <name>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub span: Span,
    pub type_id: Option<Id>,
    pub name: Option<Id>,
}

/// `native <name> takes <params> returns <return_type>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDecl {
    pub span: Span,
    pub name: Option<Id>,
    pub params: Vec<Param>,
    pub return_type: Option<Id>,
}

/// `function <name> takes <params> returns <return_type> ... endfunction`
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub span: Span,
    pub name: Option<Id>,
    pub params: Vec<Param>,
    pub return_type: Option<Id>,
    pub body: Vec<Statement>,
}

/// A single variable inside `var_stmt`: `<name> [= <value>]`
#[derive(Debug, Clone, PartialEq)]
pub struct VarInit {
    pub span: Span,
    pub name: Option<Id>,
    pub has_value: bool,
}

/// `[constant] <type> [array] <decls>`  (inside globals)
#[derive(Debug, Clone, PartialEq)]
pub struct VarStmt {
    pub span: Span,
    pub is_constant: bool,
    pub is_array: bool,
    pub type_id: Option<Id>,
    pub decls: Vec<VarInit>,
}

/// `local <type> [array] <name> [= <value>]`
#[derive(Debug, Clone, PartialEq)]
pub struct LocalDecl {
    pub span: Span,
    pub type_id: Option<Id>,
    pub name: Option<Id>,
    pub has_value: bool,
}

/// `set <variable>[<index>] = <value>`
#[derive(Debug, Clone, PartialEq)]
pub struct SetStmt {
    pub span: Span,
    pub variable: Option<Id>,
    pub has_index: bool,
}

/// `call <function_call>`
#[derive(Debug, Clone, PartialEq)]
pub struct CallStmt {
    pub span: Span,
    pub func: Option<FunctionCall>,
}

/// `<name>(<args>)`
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCall {
    pub span: Span,
    pub name: Option<Id>,
    pub arg_count: usize,
}

/// `return [<expr>]`
#[derive(Debug, Clone, PartialEq)]
pub struct ReturnStmt {
    pub span: Span,
}

/// `exitwhen <expr>`
#[derive(Debug, Clone, PartialEq)]
pub struct ExitwhenStmt {
    pub span: Span,
}

/// `if <cond> then ... [elseif ...] [else ...] endif`
#[derive(Debug, Clone, PartialEq)]
pub struct IfStmt {
    pub span: Span,
    pub body: Vec<Statement>,
}

/// `loop ... endloop`
#[derive(Debug, Clone, PartialEq)]
pub struct LoopStmt {
    pub span: Span,
    pub body: Vec<Statement>,
}

/// `globals ... endglobals`
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalsBlock {
    pub span: Span,
    pub vars: Vec<VarStmt>,
}

/// A comment line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub span: Span,
}

/// Any top-level or body statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Type(TypeDecl),
    Native(NativeDecl),
    Function(FunctionDecl),
    Globals(GlobalsBlock),
    Local(LocalDecl),
    Set(SetStmt),
    Call(CallStmt),
    Return(ReturnStmt),
    Exitwhen(ExitwhenStmt),
    If(IfStmt),
    Loop(LoopStmt),
    VarStmt(VarStmt),
    Comment(Comment),
}

/// The root of the AST.
#[derive(Debug, Clone, PartialEq)]
pub struct Ast {
    pub items: Vec<Statement>,
    pub errors: Vec<CstError>,
}

// ─── Building the AST from CST ──────────────────────────────────────────────

/// Build the AST from a tree-sitter CST root node.
/// Collects CST errors/missing nodes along the way.
pub fn build_ast(root: Node) -> Ast {
    let mut errors = Vec::new();
    let items = build_children(&root, &mut errors);
    Ast { items, errors }
}

fn collect_errors(node: &Node, errors: &mut Vec<CstError>) {
    if node.is_missing() {
        errors.push(CstError {
            span: Span::from_node(node),
            message: format!("Missing `{}`", node.kind()),
        });
    } else if node.is_error() {
        errors.push(CstError {
            span: Span::from_node(node),
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

fn build_children(node: &Node, errors: &mut Vec<CstError>) -> Vec<Statement> {
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

fn build_statement(node: &Node, errors: &mut Vec<CstError>) -> Option<Statement> {
    match Kind::try_from(node.kind_id()) {
        Ok(Kind::TypeStatement) => Some(Statement::Type(build_type_decl(node))),
        Ok(Kind::NativeStatement) => Some(Statement::Native(build_native_decl(node))),
        Ok(Kind::FunctionStatement) => Some(Statement::Function(build_function_decl(node, errors))),
        Ok(Kind::GlobalsBlock) => Some(Statement::Globals(build_globals_block(node, errors))),
        Ok(Kind::LocalStatement) => Some(Statement::Local(build_local_decl(node))),
        Ok(Kind::SetStatement) => Some(Statement::Set(build_set_stmt(node))),
        Ok(Kind::CallStatement) => Some(Statement::Call(build_call_stmt(node))),
        Ok(Kind::ReturnStatement) => Some(Statement::Return(ReturnStmt {
            span: Span::from_node(node),
        })),
        Ok(Kind::ExitwhenStatement) => Some(Statement::Exitwhen(ExitwhenStmt {
            span: Span::from_node(node),
        })),
        Ok(Kind::IfStatement) => Some(Statement::If(build_if_stmt(node, errors))),
        Ok(Kind::LoopStatement) => Some(Statement::Loop(build_loop_stmt(node, errors))),
        Ok(Kind::VarStmt) => Some(Statement::VarStmt(build_var_stmt(node))),
        Ok(Kind::Comment) => Some(Statement::Comment(Comment {
            span: Span::from_node(node),
        })),
        _ => None,
    }
}

fn build_id(node: &Node, role: IdRole) -> Id {
    Id {
        span: Span::from_node(node),
        role,
    }
}

fn maybe_id(node: &Node, field: u16, role: IdRole) -> Option<Id> {
    node.child_by_field_id(field).and_then(|n| {
        if Kind::try_from(n.kind_id()) == Ok(Kind::Id) {
            Some(build_id(&n, role))
        } else {
            None
        }
    })
}

fn build_type_decl(node: &Node) -> TypeDecl {
    TypeDecl {
        span: Span::from_node(node),
        name: maybe_id(node, FIELD_NAME, IdRole::TypeDecl),
        base: maybe_id(node, FIELD_BASE, IdRole::TypeRef),
    }
}

fn build_params(node: &Node) -> Vec<Param> {
    let mut params = Vec::new();
    if let Some(pl) = node.child_by_field_id(FIELD_PARAMETERS) {
        if Kind::try_from(pl.kind_id()) == Ok(Kind::ParameterList) {
            let count = pl.child_count();
            for i in 0..count {
                if let Some(child) = pl.child(i as u32) {
                    if Kind::try_from(child.kind_id()) == Ok(Kind::Parameter) {
                        params.push(Param {
                            span: Span::from_node(&child),
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

fn build_native_decl(node: &Node) -> NativeDecl {
    NativeDecl {
        span: Span::from_node(node),
        name: maybe_id(node, FIELD_NAME, IdRole::FunctionDecl),
        params: build_params(node),
        return_type: maybe_id(node, FIELD_RETURN_TYPE, IdRole::TypeRef),
    }
}

fn build_function_decl(node: &Node, errors: &mut Vec<CstError>) -> FunctionDecl {
    FunctionDecl {
        span: Span::from_node(node),
        name: maybe_id(node, FIELD_NAME, IdRole::FunctionDecl),
        params: build_params(node),
        return_type: maybe_id(node, FIELD_RETURN_TYPE, IdRole::TypeRef),
        body: build_children(node, errors),
    }
}

fn build_globals_block(node: &Node, errors: &mut Vec<CstError>) -> GlobalsBlock {
    let mut vars = Vec::new();
    for stmt in build_children(node, errors) {
        if let Statement::VarStmt(v) = stmt {
            vars.push(v);
        }
    }
    GlobalsBlock {
        span: Span::from_node(node),
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

fn build_var_stmt(node: &Node) -> VarStmt {
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
                    span: Span::from_node(&child),
                    name: maybe_id(&child, FIELD_NAME, var_role),
                    has_value: child.child_by_field_id(FIELD_VALUE).is_some(),
                });
            }
        }
    }

    VarStmt {
        span: Span::from_node(node),
        is_constant,
        is_array,
        type_id,
        decls,
    }
}

fn build_local_decl(node: &Node) -> LocalDecl {
    LocalDecl {
        span: Span::from_node(node),
        type_id: maybe_id(node, FIELD_TYPE, IdRole::TypeRef),
        name: maybe_id(node, FIELD_NAME, IdRole::Variable),
        has_value: node.child_by_field_id(FIELD_VALUE).is_some(),
    }
}

fn build_set_stmt(node: &Node) -> SetStmt {
    SetStmt {
        span: Span::from_node(node),
        variable: maybe_id(node, FIELD_VARIABLE, IdRole::Variable),
        has_index: node.child_by_field_id(FIELD_INDEX).is_some(),
    }
}

/// Extract the function name `id` from a `function_call` node.
/// Structure: function_call > name: expr > id
fn extract_call_name(fc_node: &Node) -> Option<Id> {
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

fn build_function_call(node: &Node) -> FunctionCall {
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
        span: Span::from_node(node),
        name: extract_call_name(node),
        arg_count,
    }
}

fn build_call_stmt(node: &Node) -> CallStmt {
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
    CallStmt {
        span: Span::from_node(node),
        func,
    }
}

fn build_if_stmt(node: &Node, errors: &mut Vec<CstError>) -> IfStmt {
    IfStmt {
        span: Span::from_node(node),
        body: build_children(node, errors),
    }
}

fn build_loop_stmt(node: &Node, errors: &mut Vec<CstError>) -> LoopStmt {
    LoopStmt {
        span: Span::from_node(node),
        body: build_children(node, errors),
    }
}

// ─── Collecting all Ids from the AST ─────────────────────────────────────────

impl Ast {
    /// Collect all `Id` nodes from the AST into a flat list.
    /// Used to build a lookup table (start_byte → role) for semantic tokens.
    pub fn collect_ids(&self) -> Vec<&Id> {
        let mut ids = Vec::new();
        for stmt in &self.items {
            collect_ids_stmt(stmt, &mut ids);
        }
        ids
    }
}

fn collect_ids_stmt<'a>(stmt: &'a Statement, ids: &mut Vec<&'a Id>) {
    match stmt {
        Statement::Type(t) => {
            if let Some(id) = &t.name { ids.push(id); }
            if let Some(id) = &t.base { ids.push(id); }
        }
        Statement::Native(n) => {
            if let Some(id) = &n.name { ids.push(id); }
            for p in &n.params {
                if let Some(id) = &p.type_id { ids.push(id); }
                if let Some(id) = &p.name { ids.push(id); }
            }
            if let Some(id) = &n.return_type { ids.push(id); }
        }
        Statement::Function(f) => {
            if let Some(id) = &f.name { ids.push(id); }
            for p in &f.params {
                if let Some(id) = &p.type_id { ids.push(id); }
                if let Some(id) = &p.name { ids.push(id); }
            }
            if let Some(id) = &f.return_type { ids.push(id); }
            for s in &f.body { collect_ids_stmt(s, ids); }
        }
        Statement::Globals(g) => {
            for v in &g.vars { collect_ids_var_stmt(v, ids); }
        }
        Statement::VarStmt(v) => {
            collect_ids_var_stmt(v, ids);
        }
        Statement::Local(l) => {
            if let Some(id) = &l.type_id { ids.push(id); }
            if let Some(id) = &l.name { ids.push(id); }
        }
        Statement::Set(s) => {
            if let Some(id) = &s.variable { ids.push(id); }
        }
        Statement::Call(c) => {
            if let Some(fc) = &c.func {
                if let Some(id) = &fc.name { ids.push(id); }
            }
        }
        Statement::If(i) => {
            for s in &i.body { collect_ids_stmt(s, ids); }
        }
        Statement::Loop(l) => {
            for s in &l.body { collect_ids_stmt(s, ids); }
        }
        Statement::Return(_) | Statement::Exitwhen(_) | Statement::Comment(_) => {}
    }
}

fn collect_ids_var_stmt<'a>(v: &'a VarStmt, ids: &mut Vec<&'a Id>) {
    if let Some(id) = &v.type_id { ids.push(id); }
    for d in &v.decls {
        if let Some(id) = &d.name { ids.push(id); }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_jass(src: &str) -> Ast {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        build_ast(tree.root_node())
    }

    fn text_of<'a>(src: &'a str, span: &Span) -> &'a str {
        &src[span.start_byte..span.end_byte]
    }

    // ── type ─────────────────────────────────────────────────────────────

    #[test]
    fn type_statement() {
        let src = "type handle extends agent\n";
        let ast = parse_jass(src);
        assert_eq!(ast.items.len(), 1);
        assert!(ast.errors.is_empty());
        match &ast.items[0] {
            Statement::Type(t) => {
                let name = t.name.as_ref().unwrap();
                assert_eq!(text_of(src, &name.span), "handle");
                assert_eq!(name.role, IdRole::TypeDecl);
                let base = t.base.as_ref().unwrap();
                assert_eq!(text_of(src, &base.span), "agent");
                assert_eq!(base.role, IdRole::TypeRef);
            }
            other => panic!("Expected Type, got {:?}", other),
        }
    }

    // ── native ───────────────────────────────────────────────────────────

    #[test]
    fn native_takes_nothing_returns_nothing() {
        let src = "native Foo takes nothing returns nothing\n";
        let ast = parse_jass(src);
        assert!(ast.errors.is_empty());
        match &ast.items[0] {
            Statement::Native(n) => {
                let name = n.name.as_ref().unwrap();
                assert_eq!(text_of(src, &name.span), "Foo");
                assert_eq!(name.role, IdRole::FunctionDecl);
                assert!(n.params.is_empty());
                assert!(n.return_type.is_none());
            }
            other => panic!("Expected Native, got {:?}", other),
        }
    }

    #[test]
    fn native_with_params() {
        let src = "native Bar takes integer a, real b returns string\n";
        let ast = parse_jass(src);
        match &ast.items[0] {
            Statement::Native(n) => {
                assert_eq!(n.params.len(), 2);
                assert_eq!(n.params[0].type_id.as_ref().unwrap().role, IdRole::TypeRef);
                assert_eq!(n.params[0].name.as_ref().unwrap().role, IdRole::Param);
                assert_eq!(text_of(src, &n.params[0].type_id.as_ref().unwrap().span), "integer");
                assert_eq!(text_of(src, &n.params[0].name.as_ref().unwrap().span), "a");
                assert_eq!(text_of(src, &n.params[1].type_id.as_ref().unwrap().span), "real");
                assert_eq!(text_of(src, &n.params[1].name.as_ref().unwrap().span), "b");
                assert_eq!(text_of(src, &n.return_type.as_ref().unwrap().span), "string");
                assert_eq!(n.return_type.as_ref().unwrap().role, IdRole::TypeRef);
            }
            other => panic!("Expected Native, got {:?}", other),
        }
    }

    // ── function ─────────────────────────────────────────────────────────

    #[test]
    fn empty_function() {
        let src = "function MyFunc takes nothing returns nothing\nendfunction\n";
        let ast = parse_jass(src);
        assert_eq!(ast.items.len(), 1);
        match &ast.items[0] {
            Statement::Function(f) => {
                assert_eq!(text_of(src, &f.name.as_ref().unwrap().span), "MyFunc");
                assert_eq!(f.name.as_ref().unwrap().role, IdRole::FunctionDecl);
                assert!(f.params.is_empty());
                assert!(f.return_type.is_none());
                assert!(f.body.is_empty());
            }
            other => panic!("Expected Function, got {:?}", other),
        }
    }

    #[test]
    fn function_with_local_and_return() {
        let src = "\
function Add takes integer a, integer b returns integer
    local integer c = a
    return c
endfunction
";
        let ast = parse_jass(src);
        match &ast.items[0] {
            Statement::Function(f) => {
                assert_eq!(f.params.len(), 2);
                assert_eq!(f.return_type.as_ref().unwrap().role, IdRole::TypeRef);
                assert_eq!(f.body.len(), 2);
                match &f.body[0] {
                    Statement::Local(l) => {
                        assert_eq!(l.type_id.as_ref().unwrap().role, IdRole::TypeRef);
                        assert_eq!(l.name.as_ref().unwrap().role, IdRole::Variable);
                        assert_eq!(text_of(src, &l.name.as_ref().unwrap().span), "c");
                        assert!(l.has_value);
                    }
                    other => panic!("Expected Local, got {:?}", other),
                }
            }
            other => panic!("Expected Function, got {:?}", other),
        }
    }

    // ── globals ──────────────────────────────────────────────────────────

    #[test]
    fn globals_block() {
        let src = "\
globals
    constant integer MAX = 100
    real x
endglobals
";
        let ast = parse_jass(src);
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
    }

    // ── call ─────────────────────────────────────────────────────────────

    #[test]
    fn call_statement_name_is_function_ref() {
        let src = "call DisplayTextToPlayer(p1, p2, p3, msg, extra, last)\n";
        let ast = parse_jass(src);
        match &ast.items[0] {
            Statement::Call(c) => {
                let fc = c.func.as_ref().expect("function_call");
                let name = fc.name.as_ref().unwrap();
                assert_eq!(text_of(src, &name.span), "DisplayTextToPlayer");
                assert_eq!(name.role, IdRole::FunctionRef);
                assert_eq!(fc.arg_count, 6);
            }
            other => panic!("Expected Call, got {:?}", other),
        }
    }

    // ── set ──────────────────────────────────────────────────────────────

    #[test]
    fn set_variable() {
        let src = "set x = 5\n";
        let ast = parse_jass(src);
        match &ast.items[0] {
            Statement::Set(s) => {
                assert_eq!(s.variable.as_ref().unwrap().role, IdRole::Variable);
                assert!(!s.has_index);
            }
            other => panic!("Expected Set, got {:?}", other),
        }
    }

    #[test]
    fn set_array_index() {
        let src = "set arr[0] = 5\n";
        let ast = parse_jass(src);
        match &ast.items[0] {
            Statement::Set(s) => {
                assert!(s.has_index);
            }
            other => panic!("Expected Set, got {:?}", other),
        }
    }

    // ── if / loop ────────────────────────────────────────────────────────

    #[test]
    fn if_statement() {
        let src = "\
function F takes nothing returns nothing
    if true then
        call Foo()
    endif
endfunction
";
        let ast = parse_jass(src);
        match &ast.items[0] {
            Statement::Function(f) => {
                match &f.body[0] {
                    Statement::If(i) => assert_eq!(i.body.len(), 1),
                    other => panic!("Expected If, got {:?}", other),
                }
            }
            other => panic!("Expected Function, got {:?}", other),
        }
    }

    #[test]
    fn loop_statement() {
        let src = "\
function F takes nothing returns nothing
    loop
        exitwhen true
    endloop
endfunction
";
        let ast = parse_jass(src);
        match &ast.items[0] {
            Statement::Function(f) => {
                match &f.body[0] {
                    Statement::Loop(l) => assert!(matches!(&l.body[0], Statement::Exitwhen(_))),
                    other => panic!("Expected Loop, got {:?}", other),
                }
            }
            other => panic!("Expected Function, got {:?}", other),
        }
    }

    // ── comments ─────────────────────────────────────────────────────────

    #[test]
    fn comments() {
        let src = "// first\n// second\n";
        let ast = parse_jass(src);
        assert_eq!(ast.items.len(), 2);
        assert!(matches!(&ast.items[0], Statement::Comment(c) if c.span.start_row == 0));
        assert!(matches!(&ast.items[1], Statement::Comment(c) if c.span.start_row == 1));
    }

    // ── errors ───────────────────────────────────────────────────────────

    #[test]
    fn collects_errors() {
        let src = "function\n";
        let ast = parse_jass(src);
        assert!(!ast.errors.is_empty(), "Should collect CST errors");
    }

    // ── collect_ids ──────────────────────────────────────────────────────

    #[test]
    fn collect_ids_roles() {
        let src = "\
type handle extends agent
native Foo takes integer x returns string
globals
    constant integer MAX = 100
endglobals
function main takes nothing returns nothing
    local integer y = 1
    set y = 2
    call Foo(y)
endfunction
";
        let ast = parse_jass(src);
        let ids = ast.collect_ids();

        let roles: Vec<(&str, IdRole)> = ids
            .iter()
            .map(|id| (text_of(src, &id.span), id.role))
            .collect();

        assert!(roles.contains(&("handle", IdRole::TypeDecl)));
        assert!(roles.contains(&("agent", IdRole::TypeRef)));
        assert!(roles.contains(&("Foo", IdRole::FunctionDecl)));
        assert!(roles.contains(&("x", IdRole::Param)));
        assert!(roles.contains(&("string", IdRole::TypeRef)));
        assert!(roles.contains(&("MAX", IdRole::Constant)));
        assert!(roles.contains(&("main", IdRole::FunctionDecl)));
        assert!(roles.contains(&("y", IdRole::Variable)));
        assert!(roles.contains(&("Foo", IdRole::FunctionRef)));
    }

    // ── full program ─────────────────────────────────────────────────────

    #[test]
    fn full_program() {
        let src = "\
type handle extends agent
native Ack takes integer m, integer n returns integer
globals
    integer g
endglobals
function main takes nothing returns nothing
    local integer x = 1
    set x = 2
    call Ack(x, x)
    if true then
        return
    endif
endfunction
";
        let ast = parse_jass(src);
        assert_eq!(ast.items.len(), 4);
        assert!(matches!(&ast.items[0], Statement::Type(_)));
        assert!(matches!(&ast.items[1], Statement::Native(_)));
        assert!(matches!(&ast.items[2], Statement::Globals(_)));
        assert!(matches!(&ast.items[3], Statement::Function(f) if f.body.len() == 4));
    }
}

