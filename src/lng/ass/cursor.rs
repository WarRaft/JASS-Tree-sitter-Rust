use std::collections::{HashMap, HashSet};

use crate::lng::ass::ast::*;
use crate::lng::ass::kind::Kind;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity};
use crate::lsp::document_symbol::lsp::{DocumentSymbol, SymbolKind};
use crate::lsp::folding::lsp::{FoldingRange, FoldingRangeKind};
use crate::lsp::semantic::hub::Hub;
use crate::lsp::semantic::lsp::Kind as TokenKind;
use crate::util::roper::node::NodeExt;
use lapce_xi_rope::Rope;
use tree_sitter::Node;

// ─── Cursor ──────────────────────────────────────────────────────────────────

/// Single-pass AST visitor that collects all LSP data.
pub struct Cursor {
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: Vec<DocumentSymbol>,
    pub folding: Vec<FoldingRange>,
    pub semantic: Hub,
    /// Per-file settings parsed from `//set key value` directives.
    pub file_settings: HashMap<String, String>,

    rope: Rope,
    id_roles: HashMap<usize, IdRole>,
    /// Start-bytes of directive nodes (//import, //set) — skipped during CST DFS.
    directive_nodes: HashSet<usize>,
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
            file_settings: HashMap::new(),
            rope: rope.clone(),
            id_roles: HashMap::new(),
            directive_nodes: HashSet::new(),
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

        // Walk AST → symbols, folding, id_roles
        c.symbols = c.visit_top_levels(&ast.items);

        // DFS CST → semantic tokens (uses id_roles built above)
        if let Some(first) = ast.items.first() {
            let root_node = Self::top_level_node(first);
            if let Some(root) = root_node.parent().or(Some(root_node)) {
                let root = Self::find_root(root);
                c.build_semantic(&root);
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

    fn top_level_node<'a>(item: &'a TopLevel) -> Node<'a> {
        match item {
            TopLevel::Include(n) => n.node,
            TopLevel::Import(n) => n.node,
            TopLevel::Namespace(n) => n.node,
            TopLevel::Typedef(n) => n.node,
            TopLevel::Funcdef(n) => n.node,
            TopLevel::Enum(n) => n.node,
            TopLevel::Interface(n) => n.node,
            TopLevel::Mixin(n) => n.node,
            TopLevel::Class(n) => n.node,
            TopLevel::Function(n) => n.node,
            TopLevel::VarDecl(n) => n.node,
            TopLevel::Comment(n) => n.node,
            TopLevel::ImportDir(n) => n.node,
            TopLevel::SetDir(n) => n.node,
            TopLevel::Other(n) => *n,
        }
    }

    // ─── Helpers ─────────────────────────────────────────────────────────

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

    // ─── Top-level visitors ──────────────────────────────────────────────

    fn visit_top_levels(&mut self, items: &[TopLevel]) -> Vec<DocumentSymbol> {
        let mut syms = Vec::new();
        for item in items {
            if let Some(sym) = self.visit_top_level(item) {
                syms.push(sym);
            }
        }
        self.flush_comment_run();
        syms
    }

    fn visit_top_level(&mut self, item: &TopLevel) -> Option<DocumentSymbol> {
        // Import directives — skip comment tracking, add dedicated semantic
        if let TopLevel::ImportDir(imp) = item {
            self.flush_comment_run();
            self.directive_nodes.insert(imp.node.start_byte());
            crate::lng::directive::visit_import_semantic(
                imp,
                &mut self.semantic,
                &mut self.diagnostics,
                &self.rope,
            );
            return None;
        }

        // SetDir directives — skip comment tracking, add dedicated semantic
        if let TopLevel::SetDir(sd) = item {
            self.flush_comment_run();
            self.directive_nodes.insert(sd.node.start_byte());
            crate::lng::directive::visit_set_semantic(
                sd,
                &mut self.semantic,
                &mut self.diagnostics,
                &mut self.file_settings,
                &self.rope,
            );
            return None;
        }

        // Comment run tracking
        if let TopLevel::Comment(c) = item {
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

        match item {
            TopLevel::Include(_) => None,
            TopLevel::Import(imp) => {
                self.register_id(&imp.module);
                None
            }
            TopLevel::Namespace(ns) => {
                self.register_id(&ns.name);
                self.push_fold_region(&ns.node);
                let children = self.visit_top_levels(&ns.body);
                Some(DocumentSymbol {
                    name: self.id_name(&ns.name),
                    kind: SymbolKind::Namespace,
                    range: ns.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&ns.name, &ns.node),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }
            TopLevel::Typedef(td) => {
                self.register_id(&td.type_id);
                self.register_id(&td.alias);
                Some(DocumentSymbol {
                    name: self.id_name(&td.alias),
                    kind: SymbolKind::TypeParameter,
                    range: td.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&td.alias, &td.node),
                    ..Default::default()
                })
            }
            TopLevel::Funcdef(fd) => {
                self.register_id(&fd.name);
                self.register_id(&fd.return_type);
                for p in &fd.params {
                    self.register_id(&p.type_id);
                    self.register_id(&p.name);
                }
                Some(DocumentSymbol {
                    name: self.id_name(&fd.name),
                    kind: SymbolKind::Function,
                    range: fd.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&fd.name, &fd.node),
                    ..Default::default()
                })
            }
            TopLevel::Enum(en) => {
                self.register_id(&en.name);
                self.push_fold_region(&en.node);
                let mut children = Vec::new();
                for m in &en.members {
                    self.register_id(&m.name);
                    if let Some(v) = &m.value {
                        self.visit_expr(v);
                    }
                    children.push(DocumentSymbol {
                        name: self.id_name(&m.name),
                        kind: SymbolKind::EnumMember,
                        range: m.node.to_range(&self.rope),
                        selection_range: self.id_sel_range(&m.name, &m.node),
                        ..Default::default()
                    });
                }
                Some(DocumentSymbol {
                    name: self.id_name(&en.name),
                    kind: SymbolKind::Enum,
                    range: en.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&en.name, &en.node),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }
            TopLevel::Interface(iface) => {
                self.register_id(&iface.name);
                self.push_fold_region(&iface.node);
                let mut children = Vec::new();
                for m in &iface.methods {
                    if let Some(sym) = self.visit_function(m) {
                        children.push(sym);
                    }
                }
                Some(DocumentSymbol {
                    name: self.id_name(&iface.name),
                    kind: SymbolKind::Interface,
                    range: iface.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&iface.name, &iface.node),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }
            TopLevel::Mixin(mx) => {
                self.register_id(&mx.name);
                self.push_fold_region(&mx.node);
                let children = self.visit_class_members(&mx.members);
                Some(DocumentSymbol {
                    name: self.id_name(&mx.name),
                    kind: SymbolKind::Class,
                    range: mx.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&mx.name, &mx.node),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }
            TopLevel::Class(cls) => {
                self.register_id(&cls.name);
                self.push_fold_region(&cls.node);
                let children = self.visit_class_members(&cls.members);
                Some(DocumentSymbol {
                    name: self.id_name(&cls.name),
                    kind: SymbolKind::Class,
                    range: cls.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&cls.name, &cls.node),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }
            TopLevel::Function(f) => self.visit_function(f),
            TopLevel::VarDecl(v) => self.visit_var_decl(v),
            TopLevel::Comment(_) => unreachable!("handled above"),
            TopLevel::ImportDir(_) => unreachable!("handled above"),
            TopLevel::SetDir(_) => unreachable!("handled above"),
            TopLevel::Other(_) => None,
        }
    }

    // ─── Class member visitors ───────────────────────────────────────────

    fn visit_class_members(&mut self, members: &[ClassMember]) -> Vec<DocumentSymbol> {
        let mut syms = Vec::new();
        for m in members {
            match m {
                ClassMember::Function(f) => {
                    if let Some(sym) = self.visit_function(f) {
                        syms.push(sym);
                    }
                }
                ClassMember::Variable(v) => {
                    if let Some(sym) = self.visit_var_decl(v) {
                        syms.push(sym);
                    }
                }
                ClassMember::Other(_) => {}
            }
        }
        syms
    }

    fn visit_function(&mut self, f: &FunctionDecl) -> Option<DocumentSymbol> {
        self.register_id(&f.name);
        self.register_id(&f.return_type);
        self.push_fold_region(&f.node);

        let mut children = Vec::new();
        for p in &f.params {
            self.register_id(&p.type_id);
            self.register_id(&p.name);
            if let Some(name_id) = &p.name {
                children.push(DocumentSymbol {
                    name: self.node_text(&name_id.node),
                    detail: p.type_id.as_ref().map(|id| self.node_text(&id.node)),
                    kind: SymbolKind::Variable,
                    range: p.node.to_range(&self.rope),
                    selection_range: name_id.node.to_range(&self.rope),
                    ..Default::default()
                });
            }
        }

        let body_syms = self.visit_stmts(&f.body);
        children.extend(body_syms);

        Some(DocumentSymbol {
            name: self.id_name(&f.name),
            kind: SymbolKind::Function,
            range: f.node.to_range(&self.rope),
            selection_range: self.id_sel_range(&f.name, &f.node),
            children: if children.is_empty() { None } else { Some(children) },
            ..Default::default()
        })
    }

    fn visit_var_decl(&mut self, v: &VarDeclStmt) -> Option<DocumentSymbol> {
        self.register_id(&v.type_id);
        for d in &v.decls {
            self.register_id(&d.name);
            if let Some(val) = &d.value {
                self.visit_expr(val);
            }
        }
        v.decls.first().map(|d| DocumentSymbol {
            name: self.id_name(&d.name),
            detail: v.type_id.as_ref().map(|id| self.node_text(&id.node)),
            kind: SymbolKind::Variable,
            range: v.node.to_range(&self.rope),
            selection_range: self.id_sel_range(&d.name, &v.node),
            ..Default::default()
        })
    }

    // ─── Statement visitors ─────────────────────────────────────────────

    fn visit_stmts(&mut self, stmts: &[Stmt]) -> Vec<DocumentSymbol> {
        let mut syms = Vec::new();
        for stmt in stmts {
            if let Some(sym) = self.visit_stmt(stmt) {
                syms.push(sym);
            }
        }
        syms
    }

    fn visit_stmt(&mut self, stmt: &Stmt) -> Option<DocumentSymbol> {
        match stmt {
            Stmt::VarDecl(v) => self.visit_var_decl(v),
            Stmt::If(i) => {
                self.push_fold_region(&i.node);
                if let Some(c) = &i.condition { self.visit_expr(c); }
                self.visit_stmts(&i.body);
                None
            }
            Stmt::While(w) => {
                self.push_fold_region(&w.node);
                if let Some(c) = &w.condition { self.visit_expr(c); }
                self.visit_stmts(&w.body);
                None
            }
            Stmt::DoWhile(d) => {
                self.push_fold_region(&d.node);
                if let Some(c) = &d.condition { self.visit_expr(c); }
                self.visit_stmts(&d.body);
                None
            }
            Stmt::For(f) => {
                self.push_fold_region(&f.node);
                self.visit_stmts(&f.body);
                None
            }
            Stmt::Foreach(f) => {
                self.push_fold_region(&f.node);
                self.visit_stmts(&f.body);
                None
            }
            Stmt::Switch(s) => {
                self.push_fold_region(&s.node);
                self.visit_stmts(&s.body);
                None
            }
            Stmt::Try(t) => {
                self.push_fold_region(&t.node);
                self.visit_stmts(&t.body);
                None
            }
            Stmt::Return(r) => {
                if let Some(v) = &r.value { self.visit_expr(v); }
                None
            }
            Stmt::Expr(e) => {
                self.visit_expr(e);
                None
            }
            Stmt::Block(stmts) => {
                self.visit_stmts(stmts);
                None
            }
            Stmt::Comment(_) | Stmt::Break(_) | Stmt::Continue(_)
            | Stmt::Throw(_) | Stmt::Other(_) => None,
        }
    }

    // ─── Expression visitor ─────────────────────────────────────────────

    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Id(id) => {
                self.id_roles.insert(id.node.start_byte(), id.role);
            }
            Expr::Call { callee, args, .. } => {
                self.register_id(callee);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            Expr::MemberAccess { object, member, .. } => {
                self.visit_expr(object);
                self.register_id(member);
            }
            Expr::NamespaceAccess { namespace, name, .. } => {
                self.register_id(namespace);
                self.register_id(name);
            }
            Expr::Subscript { object, index, .. } => {
                self.visit_expr(object);
                self.visit_expr(index);
            }
            Expr::Binary { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            Expr::Unary { operand, .. } | Expr::Postfix { operand, .. } => {
                self.visit_expr(operand);
            }
            Expr::Ternary { condition, consequence, alternative, .. } => {
                self.visit_expr(condition);
                self.visit_expr(consequence);
                self.visit_expr(alternative);
            }
            Expr::Assignment { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            Expr::Parens { inner, .. } => {
                self.visit_expr(inner);
            }
            Expr::StringLiteral(_) | Expr::NumberLiteral(_) | Expr::KeywordLiteral(_)
            | Expr::Cast { .. } | Expr::New { .. } | Expr::HandleOf { .. }
            | Expr::Lambda { .. } | Expr::Other(_) => {}
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

                // Directive comment nodes are handled in the AST pass —
                // skip them entirely so they don't get re-coloured as Comment.
                if (kind == Some(Kind::Comment) || kind == Some(Kind::BlockComment))
                    && self.directive_nodes.contains(&node.start_byte())
                {
                    if cursor.goto_next_sibling() { continue; }
                    while !cursor.goto_next_sibling() {
                        if !cursor.goto_parent() { return; }
                    }
                    continue;
                }

                // String literal: mark entire node and skip children
                if kind == Some(Kind::StringLiteral) {
                    self.semantic.add_node(&node, &self.rope, TokenKind::String, 0u32);
                    if cursor.goto_next_sibling() { continue; }
                    while !cursor.goto_next_sibling() {
                        if !cursor.goto_parent() { return; }
                    }
                    continue;
                }

                // Only leaf nodes get semantic tokens
                if node.child_count() == 0 {
                    if let Some(kind) = Kind::try_from(node.grammar_id()).ok() {
                        let token_kind = match kind {
                            Kind::Identifier => {
                                if let Some(&role) = self.id_roles.get(&node.start_byte()) {
                                    match role {
                                        IdRole::FunctionDecl | IdRole::FunctionCall
                                        | IdRole::FuncdefName => TokenKind::Function,
                                        IdRole::ClassDecl | IdRole::InterfaceDecl
                                        | IdRole::MixinDecl | IdRole::TypeRef
                                        | IdRole::TypedefAlias => TokenKind::Type,
                                        IdRole::EnumDecl => TokenKind::Enum,
                                        IdRole::EnumMember => TokenKind::EnumMember,
                                        IdRole::NamespaceDecl | IdRole::NamespaceRef => {
                                            TokenKind::Namespace
                                        }
                                        IdRole::Param => TokenKind::Parameter,
                                        IdRole::Variable => TokenKind::Variable,
                                        IdRole::Property => TokenKind::Property,
                                        IdRole::Module => TokenKind::Namespace,
                                    }
                                } else {
                                    TokenKind::Variable
                                }
                            }

                            // keywords
                            Kind::HashInclude | Kind::Import | Kind::From | Kind::Namespace
                            | Kind::Typedef | Kind::Shared | Kind::Funcdef | Kind::External
                            | Kind::Enum | Kind::Interface | Kind::Mixin | Kind::Abstract
                            | Kind::Final | Kind::Class | Kind::Private | Kind::Protected
                            | Kind::Public | Kind::Override | Kind::Explicit | Kind::Const
                            | Kind::Delete | Kind::If | Kind::Else | Kind::While | Kind::Do
                            | Kind::For | Kind::In | Kind::Switch | Kind::Case | Kind::Default
                            | Kind::Return | Kind::Break | Kind::Continue | Kind::Try
                            | Kind::Catch | Kind::Throw | Kind::Cast | Kind::OpImplCast
                            | Kind::Function | Kind::New | Kind::Is | Kind::Not | Kind::And
                            | Kind::Or | Kind::Xor | Kind::ThisExpression
                            | Kind::SuperExpression => TokenKind::Keyword,

                            // primitive type keywords
                            Kind::Void | Kind::Int | Kind::Int8 | Kind::Int16 | Kind::Int32
                            | Kind::Int64 | Kind::Uint | Kind::Uint8 | Kind::Uint16
                            | Kind::Uint32 | Kind::Uint64 | Kind::Float | Kind::Double
                            | Kind::Bool | Kind::StringKw | Kind::Auto => TokenKind::Type,

                            // literals
                            Kind::IntegerLiteral | Kind::HexLiteral | Kind::BitsLiteral
                            | Kind::FloatLiteral | Kind::NullLiteral | Kind::True
                            | Kind::False => TokenKind::Number,

                            Kind::Comment | Kind::BlockComment => {
                                // //* doc comment and //@ignore: prefix as Comment, body as String
                                let sb = node.start_byte();
                                let eb = node.end_byte();
                                let text = self.rope.slice_to_cow(sb..eb);
                                let trimmed = text.trim_start();
                                if trimmed.starts_with("//*") {
                                    let prefix_len = 3; // "//*"
                                    let ws_before = text.len() - trimmed.len();
                                    self.semantic.add_range(sb + ws_before, prefix_len, &self.rope, TokenKind::Comment, 0u32);
                                    let rest_start = sb + ws_before + prefix_len;
                                    if rest_start < eb {
                                        self.semantic.add_range(rest_start, eb - rest_start, &self.rope, TokenKind::String, 0u32);
                                    }
                                    if cursor.goto_next_sibling() { continue; }
                                    while !cursor.goto_next_sibling() {
                                        if !cursor.goto_parent() { return; }
                                    }
                                    continue;
                                } else if trimmed.starts_with("//@ignore") {
                                    let prefix_len = "//@ignore".len();
                                    let ws_before = text.len() - trimmed.len();
                                    self.semantic.add_range(sb + ws_before, prefix_len, &self.rope, TokenKind::Comment, 0u32);
                                    let rest_start = sb + ws_before + prefix_len;
                                    if rest_start < eb {
                                        self.semantic.add_range(rest_start, eb - rest_start, &self.rope, TokenKind::String, 0u32);
                                    }
                                    if cursor.goto_next_sibling() { continue; }
                                    while !cursor.goto_next_sibling() {
                                        if !cursor.goto_parent() { return; }
                                    }
                                    continue;
                                }
                                TokenKind::Comment
                            }

                            // operators & punctuation
                            Kind::LeftParen | Kind::RightParen | Kind::LeftBrace
                            | Kind::RightBrace | Kind::LeftBracket | Kind::RightBracket
                            | Kind::Comma | Kind::Equal | Kind::Colon | Kind::Tilde
                            | Kind::Question | Kind::Dot | Kind::AtDot | Kind::ColonColon
                            | Kind::At | Kind::Semicolon | Kind::PlusEq | Kind::MinusEq
                            | Kind::StarEq | Kind::SlashEq | Kind::PercentEq
                            | Kind::StarStarEq | Kind::AmpEq | Kind::PipeEq | Kind::CaretEq
                            | Kind::LtLtEq | Kind::GtGtEq | Kind::GtGtGtEq | Kind::PipePipe
                            | Kind::AmpAmp | Kind::Pipe | Kind::Caret | Kind::Amp
                            | Kind::EqEq | Kind::BangEq | Kind::Bang | Kind::Lt | Kind::Gt
                            | Kind::LtEq | Kind::GtEq | Kind::LtLt | Kind::GtGt
                            | Kind::GtGtGt | Kind::Plus | Kind::Minus | Kind::Star
                            | Kind::Slash | Kind::Percent | Kind::StarStar | Kind::PlusPlus
                            | Kind::MinusMinus => TokenKind::Operator,

                            _ => {
                                // Descend
                                if cursor.goto_first_child() { continue; }
                                if cursor.goto_next_sibling() { continue; }
                                while !cursor.goto_next_sibling() {
                                    if !cursor.goto_parent() { return; }
                                }
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

