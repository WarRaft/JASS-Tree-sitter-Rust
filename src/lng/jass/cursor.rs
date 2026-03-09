use std::collections::HashMap;

use crate::lng::jass::ast::*;
use crate::lng::jass::kind::Kind;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity};
use crate::lsp::document_symbol::lsp::{DocumentSymbol, SymbolKind};
use crate::lsp::folding::lsp::{FoldingRange, FoldingRangeKind};
use crate::lsp::semantic::hub::Hub;
use crate::lsp::semantic::lsp::Kind as TokenKind;
use crate::util::roper::node::NodeExt;
use lapce_xi_rope::Rope;
use tree_sitter::Node;

// ─── Scope types ─────────────────────────────────────────────────────────────

/// Info about a variable inside a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarInfo {
    pub start_byte: usize,
    pub type_name: Option<String>,
    pub is_array: bool,
    pub is_constant: bool,
    pub is_initialized: bool,
}

/// A completed scope snapshot.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Scope {
    pub name: String,
    pub vars: HashMap<String, VarInfo>,
}

// ─── Cursor ──────────────────────────────────────────────────────────────────

/// Single-pass AST visitor that collects all LSP data + scope info.
pub struct Cursor {
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: Vec<DocumentSymbol>,
    pub folding: Vec<FoldingRange>,
    pub semantic: Hub,
    pub scopes: Vec<Scope>,

    // Working state
    rope: Rope,
    id_roles: HashMap<usize, IdRole>,
    comment_start: Option<usize>,
    comment_end: usize,
}

impl Cursor {
    /// Walk the AST in a single pass, collecting everything.
    pub fn walk(ast: &Ast, rope: &Rope) -> Self {
        let mut c = Self {
            diagnostics: Vec::new(),
            symbols: Vec::new(),
            folding: Vec::new(),
            semantic: Hub::default(),
            scopes: Vec::new(),
            rope: rope.clone(),
            id_roles: HashMap::new(),
            comment_start: None,
            comment_end: 0,
        };

        // CST errors → diagnostics
        for e in &ast.errors {
            c.diagnostics.push(Diagnostic {
                range: e.node.to_range(rope),
                message: e.message.clone(),
                severity: Some(DiagnosticSeverity::Error),
                ..Default::default()
            });
        }

        // Walk AST → symbols, folding, scopes, id_roles
        c.symbols = c.visit_stmts(&ast.items, &mut Vec::new());

        // DFS CST → semantic tokens (uses id_roles built above)
        if let Some(first) = ast.items.first() {
            let root = Self::stmt_node(first);
            if let Some(root) = root.parent().or(Some(root)) {
                // Walk from actual root
                c.build_semantic(&Self::find_root(root));
            }
        }

        c
    }

    fn find_root(node: Node) -> Node {
        let mut n = node;
        while let Some(p) = n.parent() {
            n = p;
        }
        n
    }

    fn stmt_node<'a>(stmt: &'a Statement) -> Node<'a> {
        match stmt {
            Statement::Type(t) => t.node,
            Statement::Native(n) => n.node,
            Statement::Function(f) => f.node,
            Statement::Globals(g) => g.node,
            Statement::Local(l) => l.node,
            Statement::Set(s) => s.node,
            Statement::Call(c) => c.node,
            Statement::Return(r) => r.node,
            Statement::Exitwhen(e) => e.node,
            Statement::If(i) => i.node,
            Statement::Loop(l) => l.node,
            Statement::VarStmt(v) => v.node,
            Statement::Comment(c) => c.node,
        }
    }

    // ─── helpers ─────────────────────────────────────────────────────────

    fn node_text(&self, node: &Node) -> String {
        node.text(&self.rope).to_string()
    }

    fn push_fold_region(&mut self, node: &Node) {
        let sr = node.start_position().row;
        let er = node.end_position().row;
        if er > sr {
            self.folding.push(FoldingRange {
                start_line: sr,
                end_line: er,
                kind: Some(FoldingRangeKind::Region),
                ..Default::default()
            });
        }
    }

    fn flush_comment_run(&mut self) {
        if let Some(s) = self.comment_start.take() {
            if self.comment_end > s {
                self.folding.push(FoldingRange {
                    start_line: s,
                    end_line: self.comment_end,
                    kind: Some(FoldingRangeKind::Comment),
                    ..Default::default()
                });
            }
        }
    }

    fn register_id(&mut self, id: &Option<Id>) {
        if let Some(id) = id {
            self.id_roles.insert(id.node.start_byte(), id.role);
        }
    }

    fn id_name(&self, id: &Option<Id>) -> String {
        id.as_ref()
            .map(|id| self.node_text(&id.node))
            .unwrap_or_else(|| "<unnamed>".into())
    }

    fn id_sel_range(&self, id: &Option<Id>, fallback: &Node) -> crate::lsp::range::Range {
        id.as_ref()
            .map(|id| id.node.to_range(&self.rope))
            .unwrap_or_else(|| fallback.to_range(&self.rope))
    }

    // ─── scope helpers ───────────────────────────────────────────────────

    fn scope_define(
        vars: &mut HashMap<String, VarInfo>,
        name: &str,
        start_byte: usize,
        type_name: Option<String>,
        is_array: bool,
        is_constant: bool,
        is_initialized: bool,
    ) {
        vars.insert(
            name.to_string(),
            VarInfo { start_byte, type_name, is_array, is_constant, is_initialized },
        );
    }

    // ─── statement list visitor ──────────────────────────────────────────

    fn visit_stmts(
        &mut self,
        stmts: &[Statement],
        vars: &mut Vec<HashMap<String, VarInfo>>,
    ) -> Vec<DocumentSymbol> {
        let mut syms = Vec::new();
        for stmt in stmts {
            if let Some(sym) = self.visit_stmt(stmt, vars) {
                syms.push(sym);
            }
        }
        self.flush_comment_run();
        syms
    }

    // ─── single statement visitor ────────────────────────────────────────

    fn visit_stmt(
        &mut self,
        stmt: &Statement,
        vars: &mut Vec<HashMap<String, VarInfo>>,
    ) -> Option<DocumentSymbol> {
        // Comment tracking
        if let Statement::Comment(c) = stmt {
            let row = c.node.start_position().row;
            match self.comment_start {
                Some(_) => self.comment_end = row,
                None => {
                    self.comment_start = Some(row);
                    self.comment_end = row;
                }
            }
            return None;
        }
        self.flush_comment_run();

        match stmt {
            Statement::Type(t) => {
                self.register_id(&t.name);
                self.register_id(&t.base);
                Some(DocumentSymbol {
                    name: self.id_name(&t.name),
                    kind: SymbolKind::Class,
                    range: t.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&t.name, &t.node),
                    ..Default::default()
                })
            }

            Statement::Native(n) => {
                self.register_id(&n.name);
                for p in &n.params {
                    self.register_id(&p.type_id);
                    self.register_id(&p.name);
                }
                self.register_id(&n.return_type);
                Some(DocumentSymbol {
                    name: self.id_name(&n.name),
                    kind: SymbolKind::Interface,
                    range: n.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&n.name, &n.node),
                    ..Default::default()
                })
            }

            Statement::Function(f) => {
                self.register_id(&f.name);
                self.register_id(&f.return_type);
                self.push_fold_region(&f.node);

                let mut func_vars = HashMap::new();
                let mut children = Vec::new();

                for p in &f.params {
                    self.register_id(&p.type_id);
                    self.register_id(&p.name);
                    if let Some(name_id) = &p.name {
                        let type_name = p.type_id.as_ref().map(|id| self.node_text(&id.node));
                        Self::scope_define(
                            &mut func_vars,
                            &self.node_text(&name_id.node),
                            name_id.node.start_byte(),
                            type_name.clone(),
                            false, false, true,
                        );
                        children.push(DocumentSymbol {
                            name: self.node_text(&name_id.node),
                            detail: type_name,
                            kind: SymbolKind::Variable,
                            range: p.node.to_range(&self.rope),
                            selection_range: name_id.node.to_range(&self.rope),
                            ..Default::default()
                        });
                    }
                }

                vars.push(func_vars);
                children.extend(self.visit_stmts(&f.body, vars));
                let func_vars = vars.pop().unwrap_or_default();

                self.scopes.push(Scope {
                    name: self.id_name(&f.name),
                    vars: func_vars,
                });

                Some(DocumentSymbol {
                    name: self.id_name(&f.name),
                    kind: SymbolKind::Function,
                    range: f.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&f.name, &f.node),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }

            Statement::Globals(g) => {
                self.push_fold_region(&g.node);
                let mut children = Vec::new();
                vars.push(HashMap::new());

                for v in &g.vars {
                    self.register_id(&v.type_id);
                    let type_name = v.type_id.as_ref().map(|id| self.node_text(&id.node));

                    for d in &v.decls {
                        self.register_id(&d.name);
                        if let Some(expr) = &d.value {
                            self.visit_expr(expr);
                        }
                        if let Some(name_id) = &d.name {
                            Self::scope_define(
                                vars.last_mut().unwrap(),
                                &self.node_text(&name_id.node),
                                name_id.node.start_byte(),
                                type_name.clone(),
                                v.is_array, v.is_constant, d.value.is_some(),
                            );
                        }
                        children.push(DocumentSymbol {
                            name: self.id_name(&d.name),
                            detail: type_name.clone(),
                            kind: if v.is_constant { SymbolKind::Constant } else { SymbolKind::Variable },
                            range: d.node.to_range(&self.rope),
                            selection_range: self.id_sel_range(&d.name, &d.node),
                            ..Default::default()
                        });
                    }
                }

                let globals_vars = vars.pop().unwrap_or_default();
                self.scopes.push(Scope {
                    name: "globals".into(),
                    vars: globals_vars,
                });

                Some(DocumentSymbol {
                    name: "globals".into(),
                    kind: SymbolKind::Namespace,
                    range: g.node.to_range(&self.rope),
                    selection_range: g.node.to_range(&self.rope),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }

            Statement::Local(l) => {
                self.register_id(&l.type_id);
                self.register_id(&l.name);
                if let Some(expr) = &l.value {
                    self.visit_expr(expr);
                }
                if let (Some(scope), Some(name_id)) = (vars.last_mut(), &l.name) {
                    Self::scope_define(
                        scope,
                        &self.node_text(&name_id.node),
                        name_id.node.start_byte(),
                        l.type_id.as_ref().map(|id| self.node_text(&id.node)),
                        false, false, l.value.is_some(),
                    );
                }
                Some(DocumentSymbol {
                    name: self.id_name(&l.name),
                    detail: l.type_id.as_ref().map(|id| self.node_text(&id.node)),
                    kind: SymbolKind::Variable,
                    range: l.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&l.name, &l.node),
                    ..Default::default()
                })
            }

            Statement::VarStmt(v) => {
                self.register_id(&v.type_id);
                for d in &v.decls {
                    self.register_id(&d.name);
                    if let Some(expr) = &d.value {
                        self.visit_expr(expr);
                    }
                }
                v.decls.first().map(|d| DocumentSymbol {
                    name: self.id_name(&d.name),
                    kind: if v.is_constant { SymbolKind::Constant } else { SymbolKind::Variable },
                    range: d.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&d.name, &d.node),
                    ..Default::default()
                })
            }

            Statement::Set(s) => {
                self.register_id(&s.variable);
                if let Some(expr) = &s.index {
                    self.visit_expr(expr);
                }
                if let Some(expr) = &s.value {
                    self.visit_expr(expr);
                }
                if let Some(var_id) = &s.variable {
                    let name = self.node_text(&var_id.node);
                    for scope in vars.iter_mut().rev() {
                        if let Some(info) = scope.get_mut(&name) {
                            info.is_initialized = true;
                            break;
                        }
                    }
                }
                None
            }

            Statement::Call(c) => {
                if let Some(fc) = &c.func {
                    self.register_id(&fc.name);
                    for arg in &fc.args {
                        self.visit_expr(arg);
                    }
                }
                None
            }

            Statement::If(i) => {
                self.push_fold_region(&i.node);
                if let Some(cond) = &i.condition {
                    self.visit_expr(cond);
                }
                let _body = self.visit_stmts(&i.body, vars);
                None
            }

            Statement::Loop(l) => {
                self.push_fold_region(&l.node);
                let _body = self.visit_stmts(&l.body, vars);
                None
            }

            Statement::Return(r) => {
                if let Some(expr) = &r.value {
                    self.visit_expr(expr);
                }
                None
            }
            Statement::Exitwhen(e) => {
                if let Some(expr) = &e.condition {
                    self.visit_expr(expr);
                }
                None
            }
            Statement::Comment(_) => unreachable!("handled above"),
        }
    }

    // ─── Expression visitor ────────────────────────────────────────────

    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Id(id) => {
                self.id_roles.insert(id.node.start_byte(), id.role);
            }
            Expr::Call(fc) => {
                self.register_id(&fc.name);
                for arg in &fc.args {
                    self.visit_expr(arg);
                }
            }
            Expr::FuncRef(id) => {
                self.id_roles.insert(id.node.start_byte(), id.role);
            }
            Expr::Binary { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            Expr::Unary { operand, .. } => {
                self.visit_expr(operand);
            }
            Expr::Parens { inner, .. } => {
                self.visit_expr(inner);
            }
            Expr::Index { array, index, .. } => {
                self.visit_expr(array);
                self.visit_expr(index);
            }
            Expr::Literal(_) => {}
        }
    }

    // ─── Semantic tokens from CST DFS ────────────────────────────────────

    fn build_semantic(&mut self, root: &Node) {
        let mut cursor = root.walk();
        let mut visit = true;
        loop {
            if visit {
                let node = cursor.node();
                let kind = Kind::try_from(node.kind_id()).ok();

                // String literal: mark entire node and skip children
                if kind == Some(Kind::StringLiteral) {
                    self.semantic.add_node(&node, &self.rope, TokenKind::String, 0u32);
                    // Don't descend into children (quotes, content)
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    // Go up
                    while !cursor.goto_next_sibling() {
                        if !cursor.goto_parent() {
                            return;
                        }
                    }
                    continue;
                }

                // Only leaf nodes get semantic tokens
                if node.child_count() == 0 {
                    if let Some(kind) = Kind::try_from(node.grammar_id()).ok() {
                        let token_kind = match kind {
                            Kind::IdToken | Kind::Id => {
                                if let Some(&role) = self.id_roles.get(&node.start_byte()) {
                                    match role {
                                        IdRole::FunctionDecl | IdRole::FunctionRef => TokenKind::Function,
                                        IdRole::TypeDecl | IdRole::TypeRef => TokenKind::Type,
                                        IdRole::Param => TokenKind::Parameter,
                                        IdRole::Variable | IdRole::Constant => TokenKind::Variable,
                                    }
                                } else {
                                    TokenKind::Variable
                                }
                            }
                            Kind::Function | Kind::Endfunction | Kind::Native | Kind::Type
                            | Kind::Extends | Kind::Takes | Kind::Returns | Kind::Nothing
                            | Kind::Local | Kind::Set | Kind::Call | Kind::Return
                            | Kind::If | Kind::Then | Kind::Elseif | Kind::Else | Kind::Endif
                            | Kind::Loop | Kind::Endloop | Kind::Exitwhen
                            | Kind::Globals | Kind::Endglobals
                            | Kind::Constant | Kind::Array
                            | Kind::And | Kind::Or | Kind::Not => TokenKind::Keyword,

                            Kind::Equal | Kind::Comma
                            | Kind::LeftParen | Kind::RightParen
                            | Kind::LeftBracket | Kind::RightBracket
                            | Kind::Plus | Kind::Minus | Kind::Star | Kind::Slash
                            | Kind::PlusPlus | Kind::MinusMinus
                            | Kind::Lt | Kind::Gt | Kind::Le | Kind::Ge
                            | Kind::EqEq | Kind::Neq => TokenKind::Operator,

                            Kind::Number | Kind::Float | Kind::Rawcode => TokenKind::Number,
                            Kind::Comment => TokenKind::Comment,
                            _ => {
                                // Descend
                                if cursor.goto_first_child() { continue; }
                                #[allow(unused_assignments)]
                                { visit = false; }
                                if cursor.goto_next_sibling() { visit = true; continue; }
                                while !cursor.goto_next_sibling() {
                                    if !cursor.goto_parent() { return; }
                                }
                                visit = true;
                                continue;
                            }
                        };
                        self.semantic.add_node(&node, &self.rope, token_kind, 0u32);
                    }
                }
            }

            // DFS traversal
            if visit && cursor.goto_first_child() {
                continue;
            }
            visit = true;
            if cursor.goto_next_sibling() {
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    return;
                }
            }
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn with_cursor(src: &str, f: impl FnOnce(&Cursor)) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        let rope = Rope::from(src);
        let cursor = Cursor::walk(&ast, &rope);
        f(&cursor);
    }

    #[test]
    fn symbols_function() {
        let src = "\
function Foo takes integer x returns nothing
    local integer y = 1
endfunction
";
        with_cursor(src, |c| {
            assert_eq!(c.symbols.len(), 1);
            assert_eq!(c.symbols[0].name, "Foo");
            let ch = c.symbols[0].children.as_ref().unwrap();
            assert_eq!(ch.len(), 2);
            assert_eq!(ch[0].name, "x");
            assert_eq!(ch[1].name, "y");
        });
    }

    #[test]
    fn symbols_globals() {
        let src = "\
globals
    constant integer MAX = 100
    real x
endglobals
";
        with_cursor(src, |c| {
            assert_eq!(c.symbols.len(), 1);
            let ch = c.symbols[0].children.as_ref().unwrap();
            assert_eq!(ch[0].kind, SymbolKind::Constant);
            assert_eq!(ch[1].kind, SymbolKind::Variable);
        });
    }

    #[test]
    fn folding_regions() {
        let src = "\
function F takes nothing returns nothing
    if true then
        return
    endif
endfunction
";
        with_cursor(src, |c| {
            let regions: Vec<_> = c.folding.iter()
                .filter(|f| f.kind == Some(FoldingRangeKind::Region))
                .collect();
            assert_eq!(regions.len(), 2);
        });
    }

    #[test]
    fn folding_comments() {
        let src = "// a\n// b\n// c\ntype handle extends agent\n";
        with_cursor(src, |c| {
            let cmt: Vec<_> = c.folding.iter()
                .filter(|f| f.kind == Some(FoldingRangeKind::Comment))
                .collect();
            assert_eq!(cmt.len(), 1);
            assert_eq!(cmt[0].start_line, 0);
            assert_eq!(cmt[0].end_line, 2);
        });
    }

    #[test]
    fn scope_params_initialized() {
        let src = "\
function Foo takes integer x, real y returns nothing
endfunction
";
        with_cursor(src, |c| {
            let s = c.scopes.iter().find(|s| s.name == "Foo").unwrap();
            assert!(s.vars["x"].is_initialized);
            assert!(s.vars["y"].is_initialized);
        });
    }

    #[test]
    fn scope_local_set() {
        let src = "\
function Foo takes nothing returns nothing
    local integer x
    set x = 5
endfunction
";
        with_cursor(src, |c| {
            let s = c.scopes.iter().find(|s| s.name == "Foo").unwrap();
            assert!(s.vars["x"].is_initialized);
        });
    }

    #[test]
    fn scope_local_uninitialized() {
        let src = "\
function Foo takes nothing returns nothing
    local integer x
endfunction
";
        with_cursor(src, |c| {
            let s = c.scopes.iter().find(|s| s.name == "Foo").unwrap();
            assert!(!s.vars["x"].is_initialized);
        });
    }

    #[test]
    fn semantic_tokens_present() {
        let src = "\
function Foo takes nothing returns nothing
    call Bar()
endfunction
";
        with_cursor(src, |c| {
            let data = c.semantic.data(None);
            assert!(!data.is_empty(), "Should have semantic tokens");
        });
    }

    #[test]
    fn semantic_function_call_name_is_function() {
        // "call Foo()\n" — "Foo" starts at byte 5, row 0, col 5
        let src = "call Foo()\n";
        with_cursor(src, |c| {
            // Find the semantic token at row=0 that covers "Foo" (col=5, len=3)
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            let foo_token = line.tokens.iter().find(|t| t.col == 5 && t.len == 3);
            assert!(
                foo_token.is_some(),
                "Should have a token for 'Foo' at col=5 len=3, tokens: {:?}",
                line.tokens
            );
            assert_eq!(
                foo_token.unwrap().kind,
                TokenKind::Function,
                "Function call name should be TokenKind::Function, got {:?}",
                foo_token.unwrap().kind
            );
        });
    }

    #[test]
    fn semantic_function_call_inside_body_is_function() {
        let src = "\
function main takes nothing returns nothing
    call Foo()
endfunction
";
        with_cursor(src, |c| {
            // "Foo" is on row 1, col 9, len 3
            let line = c.semantic.lines.get(&1).expect("should have line 1");
            eprintln!("line 1 tokens: {:?}", line.tokens);
            let tok = line.tokens.iter().find(|t| t.col == 9 && t.len == 3);
            assert!(
                tok.is_some(),
                "Should have token for 'Foo' at col=9 len=3, tokens: {:?}",
                line.tokens
            );
            assert_eq!(
                tok.unwrap().kind,
                TokenKind::Function,
                "call Foo() name should be Function, got {:?}",
                tok.unwrap().kind
            );
        });
    }

    #[test]
    fn semantic_function_decl_name_is_function() {
        let src = "function MyFunc takes nothing returns nothing\nendfunction\n";
        with_cursor(src, |c| {
            // "MyFunc" starts at col=9 on row 0, len=6
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            let tok = line.tokens.iter().find(|t| t.col == 9 && t.len == 6);
            assert!(tok.is_some(), "Should have token for 'MyFunc', tokens: {:?}", line.tokens);
            assert_eq!(tok.unwrap().kind, TokenKind::Function);
        });
    }

    #[test]
    fn semantic_type_name_is_type() {
        let src = "type handle extends agent\n";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            // "handle" at col=5, len=6
            let tok = line.tokens.iter().find(|t| t.col == 5 && t.len == 6);
            assert!(tok.is_some(), "Should have token for 'handle', tokens: {:?}", line.tokens);
            assert_eq!(tok.unwrap().kind, TokenKind::Type);
            // "agent" at col=20, len=5
            let tok2 = line.tokens.iter().find(|t| t.col == 20 && t.len == 5);
            assert!(tok2.is_some(), "Should have token for 'agent', tokens: {:?}", line.tokens);
            assert_eq!(tok2.unwrap().kind, TokenKind::Type);
        });
    }

    #[test]
    fn diagnostics_from_errors() {
        let src = "function\n";
        with_cursor(src, |c| {
            assert!(!c.diagnostics.is_empty());
        });
    }

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
        with_cursor(src, |c| {
            assert_eq!(c.symbols.len(), 4);
            assert_eq!(c.scopes.len(), 2);
            assert!(!c.semantic.data(None).is_empty());
            assert!(c.diagnostics.is_empty());
        });
    }

    #[test]
    fn semantic_string_literal() {
        // 'call Foo("my shit")\n' — the string "my shit" should be String token
        let src = "call Foo(\"my shit\")\n";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            // "my shit" is at col=9, len=9 (includes quotes)
            let str_tok = line.tokens.iter().find(|t| t.col == 9);
            assert!(
                str_tok.is_some(),
                "Should have a string token at col=9, tokens: {:?}",
                line.tokens
            );
            assert_eq!(
                str_tok.unwrap().kind,
                TokenKind::String,
                "String literal should be TokenKind::String, got {:?}",
                str_tok.unwrap().kind
            );
            assert_eq!(str_tok.unwrap().len, 9, "String token len should be 9 (including quotes)");
        });
    }

    #[test]
    fn semantic_function_call_in_expression() {
        // UnitLife and IsHidden should be Function, Target should be Variable/Parameter
        let src = "\
function F takes unit Target returns boolean
    return UnitLife(Target) > 0 and not IsHidden(Target)
endfunction
";
        with_cursor(src, |c| {
            // line 1: "    return UnitLife(Target) > 0 and not IsHidden(Target)"
            let line = c.semantic.lines.get(&1).expect("should have line 1");
            // "UnitLife" col=11 len=8
            let ul_tok = line.tokens.iter().find(|t| t.col == 11 && t.len == 8);
            assert!(ul_tok.is_some(), "Should have UnitLife token, tokens: {:?}", line.tokens);
            assert_eq!(ul_tok.unwrap().kind, TokenKind::Function,
                "UnitLife should be Function, got {:?}", ul_tok.unwrap().kind);
            // "IsHidden" col=40 len=8
            let ih_tok = line.tokens.iter().find(|t| t.col == 40 && t.len == 8);
            assert!(ih_tok.is_some(), "Should have IsHidden token, tokens: {:?}", line.tokens);
            assert_eq!(ih_tok.unwrap().kind, TokenKind::Function,
                "IsHidden should be Function, got {:?}", ih_tok.unwrap().kind);
        });
    }
}

