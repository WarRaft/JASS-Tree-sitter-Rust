#![allow(dead_code)]

use crate::lng::ass::kind::{Field, Kind};
use tree_sitter::Node;

const FIELD_NAME: u16 = Field::Name as u16;
const FIELD_TYPE: u16 = Field::Type as u16;
const FIELD_BASE: u16 = Field::Base as u16;
const FIELD_BASES: u16 = Field::Bases as u16;
const FIELD_RETURN_TYPE: u16 = Field::ReturnType as u16;
const FIELD_PARAMS: u16 = Field::Params as u16;
const FIELD_BODY: u16 = Field::Body as u16;
const FIELD_VALUE: u16 = Field::Value as u16;
const FIELD_CALLEE: u16 = Field::Callee as u16;
const FIELD_ARGS: u16 = Field::Args as u16;
const FIELD_CONDITION: u16 = Field::Condition as u16;
const FIELD_CONSEQUENCE: u16 = Field::Consequence as u16;
const FIELD_ALTERNATIVE: u16 = Field::Alternative as u16;
const FIELD_LEFT: u16 = Field::Left as u16;
const FIELD_RIGHT: u16 = Field::Right as u16;
const FIELD_OPERAND: u16 = Field::Operand as u16;
const FIELD_INDEX: u16 = Field::Index as u16;
const FIELD_OBJECT: u16 = Field::Object as u16;
const FIELD_MEMBER: u16 = Field::Member as u16;
const FIELD_PATH: u16 = Field::Path as u16;
const FIELD_ITERABLE: u16 = Field::Iterable as u16;
const FIELD_UPDATE: u16 = Field::Update as u16;
const FIELD_HANDLER: u16 = Field::Handler as u16;
const FIELD_EXCEPTION: u16 = Field::Exception as u16;
const FIELD_ALIAS: u16 = Field::Alias as u16;
const FIELD_MODULE: u16 = Field::Module as u16;
const FIELD_NAMESPACE: u16 = Field::Namespace as u16;
const FIELD_OPERATOR: u16 = Field::Operator as u16;

// ─── Semantic role for identifiers ───────────────────────────────────────────

/// Describes the semantic role an identifier plays in the AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdRole {
    /// Function/method declaration name.
    FunctionDecl,
    /// Class declaration name.
    ClassDecl,
    /// Interface declaration name.
    InterfaceDecl,
    /// Enum declaration name.
    EnumDecl,
    /// Enum member name.
    EnumMember,
    /// Namespace declaration name.
    NamespaceDecl,
    /// Mixin declaration name.
    MixinDecl,
    /// Type reference (return type, parameter type, variable type, base class).
    TypeRef,
    /// Parameter name.
    Param,
    /// Variable/declarator name.
    Variable,
    /// Function/method name in a call expression.
    FunctionCall,
    /// Member access (`obj.member`).
    Property,
    /// Namespace access (`ns::name`).
    NamespaceRef,
    /// Include path / import module.
    Module,
    /// Typedef alias name.
    TypedefAlias,
    /// Funcdef name.
    FuncdefName,
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

/// `#include "path"`
#[derive(Debug, Clone)]
pub struct IncludeDirective<'tree> {
    pub node: Node<'tree>,
    pub path: Option<Node<'tree>>,
}

/// `import <module> [from <path>]`
#[derive(Debug, Clone)]
pub struct ImportDecl<'tree> {
    pub node: Node<'tree>,
    pub module: Option<Id<'tree>>,
    pub path: Option<Node<'tree>>,
}

/// `namespace <name> { ... }`
#[derive(Debug, Clone)]
pub struct NamespaceDecl<'tree> {
    pub node: Node<'tree>,
    pub name: Option<Id<'tree>>,
    pub body: Vec<TopLevel<'tree>>,
}

/// `typedef <type> <alias>`
#[derive(Debug, Clone)]
pub struct TypedefDecl<'tree> {
    pub node: Node<'tree>,
    pub type_id: Option<Id<'tree>>,
    pub alias: Option<Id<'tree>>,
}

/// `funcdef <return_type> <name>(<params>)`
#[derive(Debug, Clone)]
pub struct FuncdefDecl<'tree> {
    pub node: Node<'tree>,
    pub name: Option<Id<'tree>>,
    pub return_type: Option<Id<'tree>>,
    pub params: Vec<Param<'tree>>,
}

/// `<type> <name>`
#[derive(Debug, Clone)]
pub struct Param<'tree> {
    pub node: Node<'tree>,
    pub type_id: Option<Id<'tree>>,
    pub name: Option<Id<'tree>>,
}

/// `enum <name> { members... }`
#[derive(Debug, Clone)]
pub struct EnumDecl<'tree> {
    pub node: Node<'tree>,
    pub name: Option<Id<'tree>>,
    pub members: Vec<EnumMemberNode<'tree>>,
}

/// Single enum member: `name [= value]`
#[derive(Debug, Clone)]
pub struct EnumMemberNode<'tree> {
    pub node: Node<'tree>,
    pub name: Option<Id<'tree>>,
    pub value: Option<Expr<'tree>>,
}

/// `interface <name> [: bases] { ... }`
#[derive(Debug, Clone)]
pub struct InterfaceDecl<'tree> {
    pub node: Node<'tree>,
    pub name: Option<Id<'tree>>,
    pub methods: Vec<FunctionDecl<'tree>>,
}

/// `mixin class <name> [: bases] { ... }`
#[derive(Debug, Clone)]
pub struct MixinDecl<'tree> {
    pub node: Node<'tree>,
    pub name: Option<Id<'tree>>,
    pub members: Vec<ClassMember<'tree>>,
}

/// `class <name> [: bases] { ... }`
#[derive(Debug, Clone)]
pub struct ClassDecl<'tree> {
    pub node: Node<'tree>,
    pub name: Option<Id<'tree>>,
    pub members: Vec<ClassMember<'tree>>,
}

/// A member inside a class body.
#[derive(Debug, Clone)]
pub enum ClassMember<'tree> {
    Function(FunctionDecl<'tree>),
    Variable(VarDeclStmt<'tree>),
    Other(Node<'tree>),
}

/// `[modifiers] <return_type> <name>(<params>) { body }`
#[derive(Debug, Clone)]
pub struct FunctionDecl<'tree> {
    pub node: Node<'tree>,
    pub name: Option<Id<'tree>>,
    pub return_type: Option<Id<'tree>>,
    pub params: Vec<Param<'tree>>,
    pub body: Vec<Stmt<'tree>>,
}

/// `<type> <declarators...>;`
#[derive(Debug, Clone)]
pub struct VarDeclStmt<'tree> {
    pub node: Node<'tree>,
    pub type_id: Option<Id<'tree>>,
    pub decls: Vec<Declarator<'tree>>,
}

/// Single variable declarator: `name [= value]` or `name(args)`
#[derive(Debug, Clone)]
pub struct Declarator<'tree> {
    pub node: Node<'tree>,
    pub name: Option<Id<'tree>>,
    pub value: Option<Expr<'tree>>,
    /// Constructor-style initializer arguments: `name(arg1, arg2, ...)`
    pub args: Vec<Expr<'tree>>,
}

/// `if (cond) stmt [else stmt]`
#[derive(Debug, Clone)]
pub struct IfStmt<'tree> {
    pub node: Node<'tree>,
    pub condition: Option<Expr<'tree>>,
    pub body: Vec<Stmt<'tree>>,
}

/// `while (cond) stmt`
#[derive(Debug, Clone)]
pub struct WhileStmt<'tree> {
    pub node: Node<'tree>,
    pub condition: Option<Expr<'tree>>,
    pub body: Vec<Stmt<'tree>>,
}

/// `do { body } while (cond);`
#[derive(Debug, Clone)]
pub struct DoWhileStmt<'tree> {
    pub node: Node<'tree>,
    pub condition: Option<Expr<'tree>>,
    pub body: Vec<Stmt<'tree>>,
}

/// `for (init; cond; update) stmt`
#[derive(Debug, Clone)]
pub struct ForStmt<'tree> {
    pub node: Node<'tree>,
    pub body: Vec<Stmt<'tree>>,
}

/// `for (var : iterable) stmt`
#[derive(Debug, Clone)]
pub struct ForeachStmt<'tree> {
    pub node: Node<'tree>,
    pub body: Vec<Stmt<'tree>>,
}

/// `switch (expr) { cases... }`
#[derive(Debug, Clone)]
pub struct SwitchStmt<'tree> {
    pub node: Node<'tree>,
    pub body: Vec<Stmt<'tree>>,
}

/// `try { body } catch (exc) { handler }`
#[derive(Debug, Clone)]
pub struct TryStmt<'tree> {
    pub node: Node<'tree>,
    pub body: Vec<Stmt<'tree>>,
}

/// `return [expr];`
#[derive(Debug, Clone)]
pub struct ReturnStmt<'tree> {
    pub node: Node<'tree>,
    pub value: Option<Expr<'tree>>,
}

/// A comment (line or block).
#[derive(Debug, Clone)]
pub struct Comment<'tree> {
    pub node: Node<'tree>,
}

/// Statement in a function body.
#[derive(Debug, Clone)]
pub enum Stmt<'tree> {
    VarDecl(VarDeclStmt<'tree>),
    If(IfStmt<'tree>),
    While(WhileStmt<'tree>),
    DoWhile(DoWhileStmt<'tree>),
    For(ForStmt<'tree>),
    Foreach(ForeachStmt<'tree>),
    Switch(SwitchStmt<'tree>),
    Try(TryStmt<'tree>),
    Return(ReturnStmt<'tree>),
    Break(Node<'tree>),
    Continue(Node<'tree>),
    Throw(Node<'tree>),
    Expr(Expr<'tree>),
    Comment(Comment<'tree>),
    Block(Vec<Stmt<'tree>>),
    Other(Node<'tree>),
}

// ─── Expressions ─────────────────────────────────────────────────────────────

/// Expression node in the AST.
#[derive(Debug, Clone)]
pub enum Expr<'tree> {
    /// Plain identifier reference.
    Id(Id<'tree>),
    /// `callee(args...)`
    Call {
        node: Node<'tree>,
        callee: Option<Id<'tree>>,
        /// Namespace-qualified callee: `Ns::Func(args...)`.
        /// Set when the callee is a `NamespaceAccess` node.
        callee_expr: Option<Box<Expr<'tree>>>,
        args: Vec<Expr<'tree>>,
    },
    /// `obj.member`
    MemberAccess {
        node: Node<'tree>,
        object: Box<Expr<'tree>>,
        member: Option<Id<'tree>>,
    },
    /// `ns::name`
    NamespaceAccess {
        node: Node<'tree>,
        namespace: Option<Id<'tree>>,
        name: Option<Id<'tree>>,
    },
    /// `expr[index]`
    Subscript {
        node: Node<'tree>,
        object: Box<Expr<'tree>>,
        index: Box<Expr<'tree>>,
    },
    /// `left OP right`
    Binary {
        node: Node<'tree>,
        left: Box<Expr<'tree>>,
        right: Box<Expr<'tree>>,
    },
    /// Unary: `!expr`, `-expr`, `++expr`, `--expr`
    Unary {
        node: Node<'tree>,
        operand: Box<Expr<'tree>>,
    },
    /// Postfix: `expr++`, `expr--`
    Postfix {
        node: Node<'tree>,
        operand: Box<Expr<'tree>>,
    },
    /// `cond ? then : else`
    Ternary {
        node: Node<'tree>,
        condition: Box<Expr<'tree>>,
        consequence: Box<Expr<'tree>>,
        alternative: Box<Expr<'tree>>,
    },
    /// `left = right` (assignment)
    Assignment {
        node: Node<'tree>,
        left: Box<Expr<'tree>>,
        right: Box<Expr<'tree>>,
    },
    /// `cast<type>(expr)`
    Cast { node: Node<'tree> },
    /// `new Type(...)`
    New { node: Node<'tree> },
    /// `(expr)`
    Parens {
        node: Node<'tree>,
        inner: Box<Expr<'tree>>,
    },
    /// `@handle` or `@FuncName` — function/handle reference
    HandleOf {
        node: Node<'tree>,
        operand: Box<Expr<'tree>>,
    },
    /// Lambda: `function(params) { ... }`
    Lambda { node: Node<'tree> },
    /// String literal.
    StringLiteral(Node<'tree>),
    /// Numeric literal (int, hex, bits, float).
    NumberLiteral(Node<'tree>),
    /// `null`, `true`, `false`, `this`, `super`
    KeywordLiteral(Node<'tree>),
    /// Anything else we don't specifically handle.
    Other(Node<'tree>),
}

// Re-export shared directive types for convenience.
pub use crate::lng::directive::{EntryDirective, IgnoreDirective, ImportDirective, SetDirective, UjapiDirective};

/// Top-level statement (script level or inside namespace).
#[derive(Debug, Clone)]
pub enum TopLevel<'tree> {
    Include(IncludeDirective<'tree>),
    Import(ImportDecl<'tree>),
    Namespace(NamespaceDecl<'tree>),
    Typedef(TypedefDecl<'tree>),
    Funcdef(FuncdefDecl<'tree>),
    Enum(EnumDecl<'tree>),
    Interface(InterfaceDecl<'tree>),
    Mixin(MixinDecl<'tree>),
    Class(ClassDecl<'tree>),
    Function(FunctionDecl<'tree>),
    VarDecl(VarDeclStmt<'tree>),
    Comment(Comment<'tree>),
    /// `//import` / `//import!` directive (shared with JASS).
    ImportDir(ImportDirective<'tree>),
    /// `//set key value` directive (shared with JASS).
    SetDir(SetDirective<'tree>),
    /// `//ignore tag…` directive (shared with JASS).
    IgnoreDir(IgnoreDirective<'tree>),
    /// `//import-ujapi! <path>` directive (shared with JASS).
    UjapiDir(UjapiDirective<'tree>),
    /// `//entry` directive (shared with JASS) — marks the file as a build entry point.
    EntryDir(EntryDirective<'tree>),
    Other(Node<'tree>),
}

/// The root of the AST.
#[derive(Debug, Clone)]
pub struct Ast<'tree> {
    pub items: Vec<TopLevel<'tree>>,
    pub errors: Vec<CstError<'tree>>,
}

// ─── Building the AST from CST ──────────────────────────────────────────────

/// Build the AST from a tree-sitter CST root node.
///
/// Root-level `//import` / `//import!` / `//set` comments are **not** rewritten
/// here — call [`rewrite_directives`] afterwards with the source bytes.
pub fn build_ast(root: Node) -> Ast {
    let mut errors = Vec::new();
    let items = build_top_level_children(&root, &mut errors);
    Ast { items, errors }
}

/// Rewrite leading root-level comments into `TopLevel::ImportDir` or
/// `TopLevel::SetDir` when they match the `//import` / `//import!` /
/// `//set` patterns.
///
/// Only comments **before the first non-comment statement** are considered.
///
/// `src` — full file source (UTF-8 bytes).
pub fn rewrite_directives(ast: &mut Ast, src: &[u8]) {
    use crate::lng::directive::{try_parse_directive, Directive};

    let mut i = 0;
    while i < ast.items.len() {
        match &ast.items[i] {
            TopLevel::Comment(_) => {}
            TopLevel::ImportDir(_) | TopLevel::SetDir(_) | TopLevel::IgnoreDir(_) | TopLevel::UjapiDir(_) | TopLevel::EntryDir(_) => {
                i += 1;
                continue;
            }
            _ => break,
        }

        if let TopLevel::Comment(c) = &ast.items[i] {
            if let Some(dir) = try_parse_directive(&c.node, src) {
                ast.items[i] = match dir {
                    Directive::Import(imp) => TopLevel::ImportDir(imp),
                    Directive::Set(sd) => TopLevel::SetDir(sd),
                    Directive::Ignore(ig) => TopLevel::IgnoreDir(ig),
                    Directive::Ujapi(ud) => TopLevel::UjapiDir(ud),
                    Directive::Entry(ed) => TopLevel::EntryDir(ed),
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

fn build_top_level_children<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> Vec<TopLevel<'tree>> {
    let mut items = Vec::new();
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if child.is_error() || child.is_missing() {
                collect_errors(&child, errors);
            }
            items.push(build_top_level(&child, errors));
        }
    }
    items
}

fn build_top_level<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> TopLevel<'tree> {
    match Kind::try_from(node.kind_id()) {
        Ok(Kind::IncludeDirective) => TopLevel::Include(build_include(node)),
        Ok(Kind::ImportDeclaration) => TopLevel::Import(build_import(node)),
        Ok(Kind::NamespaceDeclaration) => TopLevel::Namespace(build_namespace(node, errors)),
        Ok(Kind::TypedefDeclaration) => TopLevel::Typedef(build_typedef(node)),
        Ok(Kind::FuncdefDeclaration) => TopLevel::Funcdef(build_funcdef(node)),
        Ok(Kind::EnumDeclaration) => TopLevel::Enum(build_enum(node)),
        Ok(Kind::InterfaceDeclaration) => TopLevel::Interface(build_interface(node, errors)),
        Ok(Kind::MixinDeclaration) => TopLevel::Mixin(build_mixin(node, errors)),
        Ok(Kind::ClassDeclaration) => TopLevel::Class(build_class(node, errors)),
        Ok(Kind::FunctionDeclaration) => TopLevel::Function(build_function(node, errors)),
        Ok(Kind::VariableDeclarationStatement) => TopLevel::VarDecl(build_var_decl_stmt(node)),
        Ok(Kind::Comment) | Ok(Kind::BlockComment) => TopLevel::Comment(Comment { node: *node }),
        _ => TopLevel::Other(*node),
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn maybe_id<'tree>(node: &Node<'tree>, field: u16, role: IdRole) -> Option<Id<'tree>> {
    node.child_by_field_id(field).map(|n| Id { node: n, role })
}

fn maybe_type_id<'tree>(node: &Node<'tree>) -> Option<Id<'tree>> {
    node.child_by_field_id(FIELD_TYPE).map(|n| Id {
        node: n,
        role: IdRole::TypeRef,
    })
}

fn build_params<'tree>(node: &Node<'tree>) -> Vec<Param<'tree>> {
    let mut params = Vec::new();
    if let Some(pl) = node.child_by_field_id(FIELD_PARAMS) {
        let count = pl.child_count();
        for i in 0..count {
            if let Some(child) = pl.child(i as u32) {
                if Kind::try_from(child.kind_id()) == Ok(Kind::Parameter) {
                    params.push(Param {
                        node: child,
                        type_id: maybe_type_id(&child),
                        name: maybe_id(&child, FIELD_NAME, IdRole::Param),
                    });
                }
            }
        }
    }
    params
}

fn build_body_stmts<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> Vec<Stmt<'tree>> {
    let mut stmts = Vec::new();
    // First try field_body, then look for a Block child node
    let body = node.child_by_field_id(FIELD_BODY).or_else(|| {
        let count = node.child_count();
        for i in 0..count {
            if let Some(child) = node.child(i as u32) {
                if Kind::try_from(child.kind_id()) == Ok(Kind::Block) {
                    return Some(child);
                }
            }
        }
        None
    });
    if let Some(body) = body {
        let count = body.child_count();
        for i in 0..count {
            if let Some(child) = body.child(i as u32) {
                if child.is_error() || child.is_missing() {
                    collect_errors(&child, errors);
                }
                stmts.push(build_stmt(&child, errors));
            }
        }
    }
    stmts
}

fn build_block_stmts<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> Vec<Stmt<'tree>> {
    let mut stmts = Vec::new();
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if child.is_error() || child.is_missing() {
                collect_errors(&child, errors);
            }
            stmts.push(build_stmt(&child, errors));
        }
    }
    stmts
}

// ─── Top-level builders ─────────────────────────────────────────────────────

fn build_include<'tree>(node: &Node<'tree>) -> IncludeDirective<'tree> {
    IncludeDirective {
        node: *node,
        path: node.child_by_field_id(FIELD_PATH),
    }
}

fn build_import<'tree>(node: &Node<'tree>) -> ImportDecl<'tree> {
    ImportDecl {
        node: *node,
        module: maybe_id(node, FIELD_MODULE, IdRole::Module),
        path: node.child_by_field_id(FIELD_PATH),
    }
}

fn build_namespace<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> NamespaceDecl<'tree> {
    let mut body = Vec::new();
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if child.is_error() || child.is_missing() {
                collect_errors(&child, errors);
            }
            let kind = Kind::try_from(child.kind_id());
            // Skip braces, keywords, name — only process body statements
            if matches!(kind, Ok(k) if k != Kind::Namespace && k != Kind::LeftBrace
                && k != Kind::RightBrace && k != Kind::Identifier)
            {
                body.push(build_top_level(&child, errors));
            }
        }
    }
    NamespaceDecl {
        node: *node,
        name: maybe_id(node, FIELD_NAME, IdRole::NamespaceDecl),
        body,
    }
}

fn build_typedef<'tree>(node: &Node<'tree>) -> TypedefDecl<'tree> {
    TypedefDecl {
        node: *node,
        type_id: maybe_type_id(node),
        alias: maybe_id(node, FIELD_ALIAS, IdRole::TypedefAlias),
    }
}

fn build_funcdef<'tree>(node: &Node<'tree>) -> FuncdefDecl<'tree> {
    FuncdefDecl {
        node: *node,
        name: maybe_id(node, FIELD_NAME, IdRole::FuncdefName),
        return_type: node.child_by_field_id(FIELD_RETURN_TYPE).map(|n| Id {
            node: n,
            role: IdRole::TypeRef,
        }),
        params: build_params(node),
    }
}

fn build_enum<'tree>(node: &Node<'tree>) -> EnumDecl<'tree> {
    let mut members = Vec::new();
    // enum_body is inside
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if Kind::try_from(child.kind_id()) == Ok(Kind::EnumBody) {
                let mc = child.child_count();
                for j in 0..mc {
                    if let Some(m) = child.child(j as u32) {
                        if Kind::try_from(m.kind_id()) == Ok(Kind::EnumMember) {
                            members.push(EnumMemberNode {
                                node: m,
                                name: maybe_id(&m, FIELD_NAME, IdRole::EnumMember),
                                value: m
                                    .child_by_field_id(FIELD_VALUE)
                                    .and_then(|n| build_expr(&n)),
                            });
                        }
                    }
                }
            }
        }
    }
    EnumDecl {
        node: *node,
        name: maybe_id(node, FIELD_NAME, IdRole::EnumDecl),
        members,
    }
}

fn build_interface<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> InterfaceDecl<'tree> {
    let mut methods = Vec::new();
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if Kind::try_from(child.kind_id()) == Ok(Kind::InterfaceMethod) {
                methods.push(build_function(&child, errors));
            }
        }
    }
    InterfaceDecl {
        node: *node,
        name: maybe_id(node, FIELD_NAME, IdRole::InterfaceDecl),
        methods,
    }
}

fn build_class_members<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> Vec<ClassMember<'tree>> {
    let mut members = Vec::new();
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            if child.is_error() || child.is_missing() {
                collect_errors(&child, errors);
            }
            match Kind::try_from(child.kind_id()) {
                Ok(Kind::FunctionDeclaration) => {
                    members.push(ClassMember::Function(build_function(&child, errors)));
                }
                Ok(Kind::VariableDeclarationStatement) => {
                    members.push(ClassMember::Variable(build_var_decl_stmt(&child)));
                }
                _ => {}
            }
        }
    }
    members
}

fn build_mixin<'tree>(node: &Node<'tree>, errors: &mut Vec<CstError<'tree>>) -> MixinDecl<'tree> {
    MixinDecl {
        node: *node,
        name: maybe_id(node, FIELD_NAME, IdRole::MixinDecl),
        members: build_class_members(node, errors),
    }
}

fn build_class<'tree>(node: &Node<'tree>, errors: &mut Vec<CstError<'tree>>) -> ClassDecl<'tree> {
    ClassDecl {
        node: *node,
        name: maybe_id(node, FIELD_NAME, IdRole::ClassDecl),
        members: build_class_members(node, errors),
    }
}

fn build_function<'tree>(
    node: &Node<'tree>,
    errors: &mut Vec<CstError<'tree>>,
) -> FunctionDecl<'tree> {
    FunctionDecl {
        node: *node,
        name: maybe_id(node, FIELD_NAME, IdRole::FunctionDecl),
        return_type: node.child_by_field_id(FIELD_RETURN_TYPE).map(|n| Id {
            node: n,
            role: IdRole::TypeRef,
        }),
        params: build_params(node),
        body: build_body_stmts(node, errors),
    }
}

fn build_arg_list<'tree>(node: &Node<'tree>) -> Vec<Expr<'tree>> {
    let mut args = Vec::new();
    if let Some(al) = node.child_by_field_id(FIELD_ARGS) {
        let count = al.child_count();
        for i in 0..count {
            if let Some(child) = al.child(i as u32) {
                if let Some(e) = build_expr(&child) {
                    args.push(e);
                }
            }
        }
    }
    args
}

fn build_declarator<'tree>(d: &Node<'tree>) -> Declarator<'tree> {
    Declarator {
        node: *d,
        name: maybe_id(d, FIELD_NAME, IdRole::Variable),
        value: d.child_by_field_id(FIELD_VALUE).and_then(|n| build_expr(&n)),
        args: build_arg_list(d),
    }
}

fn build_var_decl_stmt<'tree>(node: &Node<'tree>) -> VarDeclStmt<'tree> {
    let mut decls = Vec::new();
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i as u32) {
            match Kind::try_from(child.kind_id()) {
                Ok(Kind::VariableDeclaration) => {
                    // variable_declaration has type + declarator children
                    let vc = child.child_count();
                    for j in 0..vc {
                        if let Some(d) = child.child(j as u32) {
                            if Kind::try_from(d.kind_id()) == Ok(Kind::Declarator) {
                                decls.push(build_declarator(&d));
                            }
                        }
                    }
                    return VarDeclStmt {
                        node: *node,
                        type_id: maybe_type_id(&child),
                        decls,
                    };
                }
                Ok(Kind::Declarator) => {
                    decls.push(build_declarator(&child));
                }
                _ => {}
            }
        }
    }
    VarDeclStmt {
        node: *node,
        type_id: maybe_type_id(node),
        decls,
    }
}

// ─── Statement builder ──────────────────────────────────────────────────────

fn build_stmt<'tree>(node: &Node<'tree>, errors: &mut Vec<CstError<'tree>>) -> Stmt<'tree> {
    match Kind::try_from(node.kind_id()) {
        Ok(Kind::VariableDeclarationStatement) => Stmt::VarDecl(build_var_decl_stmt(node)),
        Ok(Kind::IfStatement) => Stmt::If(IfStmt {
            node: *node,
            condition: node
                .child_by_field_id(FIELD_CONDITION)
                .and_then(|n| build_expr(&n)),
            body: build_block_stmts(node, errors),
        }),
        Ok(Kind::WhileStatement) => Stmt::While(WhileStmt {
            node: *node,
            condition: node
                .child_by_field_id(FIELD_CONDITION)
                .and_then(|n| build_expr(&n)),
            body: build_block_stmts(node, errors),
        }),
        Ok(Kind::DoWhileStatement) => Stmt::DoWhile(DoWhileStmt {
            node: *node,
            condition: node
                .child_by_field_id(FIELD_CONDITION)
                .and_then(|n| build_expr(&n)),
            body: build_block_stmts(node, errors),
        }),
        Ok(Kind::ForStatement) => Stmt::For(ForStmt {
            node: *node,
            body: build_block_stmts(node, errors),
        }),
        Ok(Kind::ForeachStatement) => Stmt::Foreach(ForeachStmt {
            node: *node,
            body: build_block_stmts(node, errors),
        }),
        Ok(Kind::SwitchStatement) => Stmt::Switch(SwitchStmt {
            node: *node,
            body: build_block_stmts(node, errors),
        }),
        Ok(Kind::TryStatement) => Stmt::Try(TryStmt {
            node: *node,
            body: build_block_stmts(node, errors),
        }),
        Ok(Kind::ReturnStatement) => Stmt::Return(ReturnStmt {
            node: *node,
            value: node
                .child_by_field_id(FIELD_VALUE)
                .or_else(|| {
                    // Return statement may have a direct expression child
                    let count = node.child_count();
                    for i in 0..count {
                        if let Some(c) = node.child(i as u32) {
                            if Kind::try_from(c.kind_id()) == Ok(Kind::Expression) {
                                return Some(c);
                            }
                        }
                    }
                    None
                })
                .and_then(|n| build_expr(&n)),
        }),
        Ok(Kind::BreakStatement) => Stmt::Break(*node),
        Ok(Kind::ContinueStatement) => Stmt::Continue(*node),
        Ok(Kind::ThrowStatement) => Stmt::Throw(*node),
        Ok(Kind::ExpressionStatement) => {
            let mut expr = None;
            let count = node.child_count();
            for i in 0..count {
                if let Some(child) = node.child(i as u32) {
                    if let Some(e) = build_expr(&child) {
                        expr = Some(e);
                        break;
                    }
                }
            }
            expr.map(Stmt::Expr).unwrap_or(Stmt::Other(*node))
        }
        Ok(Kind::Block) => Stmt::Block(build_block_stmts(node, errors)),
        Ok(Kind::Comment) | Ok(Kind::BlockComment) => Stmt::Comment(Comment { node: *node }),
        _ => Stmt::Other(*node),
    }
}

// ─── Expression builder ────────────────────────────────────────────────────��

fn build_expr<'tree>(node: &Node<'tree>) -> Option<Expr<'tree>> {
    let kind = Kind::try_from(node.kind_id()).ok()?;
    match kind {
        Kind::Expression => {
            // Expression wraps a single child
            let count = node.child_count();
            for i in 0..count {
                if let Some(child) = node.child(i as u32) {
                    if let Some(e) = build_expr(&child) {
                        return Some(e);
                    }
                }
            }
            None
        }
        Kind::Identifier => Some(Expr::Id(Id {
            node: *node,
            role: IdRole::Variable,
        })),
        Kind::FunctionCall => {
            let callee_node = node.child_by_field_id(FIELD_CALLEE);
            // The callee may be wrapped in an Expression node — unwrap it.
            let callee_inner = callee_node.and_then(|cn| {
                if Kind::try_from(cn.kind_id()) == Ok(Kind::Expression) {
                    let count = cn.child_count();
                    for i in 0..count {
                        if let Some(child) = cn.child(i as u32) {
                            if child.is_named() {
                                return Some(child);
                            }
                        }
                    }
                    Some(cn)
                } else {
                    Some(cn)
                }
            });
            let (callee, callee_expr) = match callee_inner {
                Some(cn) if Kind::try_from(cn.kind_id()) == Ok(Kind::NamespaceAccess) => {
                    (None, build_expr(&cn).map(Box::new))
                }
                _ => {
                    (callee_inner.map(|n| Id { node: n, role: IdRole::FunctionCall }), None)
                }
            };
            let mut args = Vec::new();
            if let Some(al) = node.child_by_field_id(FIELD_ARGS) {
                let count = al.child_count();
                for i in 0..count {
                    if let Some(child) = al.child(i as u32) {
                        if let Some(e) = build_expr(&child) {
                            args.push(e);
                        }
                    }
                }
            }
            Some(Expr::Call {
                node: *node,
                callee,
                callee_expr,
                args,
            })
        }
        Kind::MemberAccess => {
            let object = node
                .child_by_field_id(FIELD_OBJECT)
                .and_then(|n| build_expr(&n))
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));
            let member = maybe_id(node, FIELD_MEMBER, IdRole::Property);
            Some(Expr::MemberAccess {
                node: *node,
                object,
                member,
            })
        }
        Kind::NamespaceAccess => Some(Expr::NamespaceAccess {
            node: *node,
            namespace: maybe_id(node, FIELD_NAMESPACE, IdRole::NamespaceRef),
            name: maybe_id(node, FIELD_MEMBER, IdRole::Variable),
        }),
        Kind::SubscriptExpression => {
            let object = node
                .child_by_field_id(FIELD_OBJECT)
                .and_then(|n| build_expr(&n))
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));
            let index = node
                .child_by_field_id(FIELD_INDEX)
                .and_then(|n| build_expr(&n))
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));
            Some(Expr::Subscript {
                node: *node,
                object,
                index,
            })
        }
        Kind::BinaryExpression => {
            let left = node
                .child_by_field_id(FIELD_LEFT)
                .and_then(|n| build_expr(&n))
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));
            let right = node
                .child_by_field_id(FIELD_RIGHT)
                .and_then(|n| build_expr(&n))
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));
            Some(Expr::Binary {
                node: *node,
                left,
                right,
            })
        }
        Kind::UnaryExpression => {
            // `@expr` → HandleOf (function/handle reference)
            let is_at = node
                .child_by_field_id(FIELD_OPERATOR)
                .map(|op| Kind::try_from(op.kind_id()) == Ok(Kind::At))
                .unwrap_or(false);

            let operand = node
                .child_by_field_id(FIELD_OPERAND)
                .and_then(|n| build_expr(&n))
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));

            if is_at {
                Some(Expr::HandleOf {
                    node: *node,
                    operand,
                })
            } else {
                Some(Expr::Unary {
                    node: *node,
                    operand,
                })
            }
        }
        Kind::PostfixExpression => {
            let operand = node
                .child_by_field_id(FIELD_OPERAND)
                .and_then(|n| build_expr(&n))
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));
            Some(Expr::Postfix {
                node: *node,
                operand,
            })
        }
        Kind::TernaryExpression => {
            let condition = node
                .child_by_field_id(FIELD_CONDITION)
                .and_then(|n| build_expr(&n))
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));
            let consequence = node
                .child_by_field_id(FIELD_CONSEQUENCE)
                .and_then(|n| build_expr(&n))
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));
            let alternative = node
                .child_by_field_id(FIELD_ALTERNATIVE)
                .and_then(|n| build_expr(&n))
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));
            Some(Expr::Ternary {
                node: *node,
                condition,
                consequence,
                alternative,
            })
        }
        Kind::AssignmentExpression => {
            let left = node
                .child_by_field_id(FIELD_LEFT)
                .and_then(|n| build_expr(&n))
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));
            let right = node
                .child_by_field_id(FIELD_RIGHT)
                .and_then(|n| build_expr(&n))
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));
            Some(Expr::Assignment {
                node: *node,
                left,
                right,
            })
        }
        Kind::CastExpression => Some(Expr::Cast { node: *node }),
        Kind::NewExpression => Some(Expr::New { node: *node }),
        Kind::ParenthesizedExpression => {
            let mut inner = None;
            let count = node.child_count();
            for i in 0..count {
                if let Some(child) = node.child(i as u32) {
                    if let Some(e) = build_expr(&child) {
                        inner = Some(e);
                        break;
                    }
                }
            }
            let inner = inner
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));
            Some(Expr::Parens { node: *node, inner })
        }
        Kind::HandleOfExpression => {
            let operand = node
                .child_by_field_id(FIELD_OPERAND)
                .and_then(|n| build_expr(&n))
                .map(Box::new)
                .unwrap_or_else(|| Box::new(Expr::Other(*node)));
            Some(Expr::HandleOf { node: *node, operand })
        }
        Kind::LambdaExpression => Some(Expr::Lambda { node: *node }),
        Kind::StringLiteral => Some(Expr::StringLiteral(*node)),
        Kind::IntegerLiteral | Kind::HexLiteral | Kind::BitsLiteral | Kind::FloatLiteral
        | Kind::CharLiteral => {
            Some(Expr::NumberLiteral(*node))
        }
        Kind::PrimaryExpression => {
            // primary_expression wraps a single child
            let count = node.child_count();
            for i in 0..count {
                if let Some(child) = node.child(i as u32) {
                    if let Some(e) = build_expr(&child) {
                        return Some(e);
                    }
                }
            }
            Some(Expr::Other(*node))
        }
        Kind::ExpressionList => {
            // Take first expression
            let count = node.child_count();
            for i in 0..count {
                if let Some(child) = node.child(i as u32) {
                    if let Some(e) = build_expr(&child) {
                        return Some(e);
                    }
                }
            }
            None
        }
        Kind::ScopedName => {
            // scoped_name: ns::ns::name — treat as identifier
            Some(Expr::Id(Id {
                node: *node,
                role: IdRole::TypeRef,
            }))
        }
        Kind::InitializerList => Some(Expr::Other(*node)),
        _ => None,
    }
}
