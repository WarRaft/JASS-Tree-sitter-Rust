use std::collections::{HashMap, HashSet};
use crate::http::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::http::semantic::hub::Hub;
use crate::lng::jass::ast::{Ast, Statement};
use crate::lng::jass::type_map::TypeMap;
use crate::lng::symbol::FileSymbols;
use crate::util::roper::node::NodeExt;
use lapce_xi_rope::Rope;
use tree_sitter::Node;
use super::{Cursor, HlScope, ImportedKind, ImportedSymbol};

impl Cursor {
    /// Walk the AST in two phases, collecting everything the LSP needs.
    pub fn walk(ast: &Ast, rope: &Rope, imported: &[ImportedSymbol]) -> Self {
        let mut c = Self {
            diagnostics: Vec::new(),
            symbols: Vec::new(),
            folding: Vec::new(),
            semantic: Hub::default(),
            scopes: Vec::new(),
            file_symbols: FileSymbols::new_jass(),
            ref_groups: HashMap::new(),
            ref_names: HashMap::new(),
            external_decls: HashMap::new(),
            func_decl_keys: HashSet::new(),
            var_decl_keys: HashSet::new(),
            arg_decl_keys: HashSet::new(),
            colors: Vec::new(),
            file_settings: HashMap::new(),
            file_ignore_tags: HashSet::new(),
            type_map: TypeMap::default(),
            type_hints: Vec::new(),
            comptime_values: HashMap::new(),
            ast_comptime_values: ast.comptime_values.clone(),
            rope: rope.clone(),
            id_roles: HashMap::new(),
            directive_nodes: HashSet::new(),
            comment_start: None,
            comment_end: 0,
            decl_counter: 0,
            next_decl_key: 0,
            current_callees: None,
            bare_callees: HashSet::new(),
            hl_scopes: vec![HlScope::default()],
            unresolved_refs: Vec::new(),
            imported_func_returns: HashMap::new(),
            imported_var_types: HashMap::new(),
            current_return_type: None,
        };

        // Pre-populate imported type lookup maps for Phase 1 type inference.
        for sym in imported {
            match sym.kind {
                ImportedKind::Func => {
                    c.imported_func_returns
                        .entry(sym.name.clone())
                        .or_insert_with(|| sym.return_type.clone());
                }
                ImportedKind::Var => {
                    c.imported_var_types
                        .entry(sym.name.clone())
                        .or_insert_with(|| sym.type_name.clone());
                }
            }
        }

        // CST errors → diagnostics
        for e in &ast.errors {
            c.diagnostics.push(Diagnostic {
                range: e.node.to_range(rope),
                message: e.message.clone(),
                severity: Some(DiagnosticSeverity::Error),
                ..Diagnostic::new("jass", "syntax")
            });
        }

        // Phase 1: Walk AST with only local scopes
        c.symbols = c.visit_stmts(&ast.items, &mut Vec::new());

        // Phase 2: Link unresolved refs against imported symbols
        c.link_imports(imported);

        // DFS CST → semantic tokens (uses id_roles built above)
        if let Some(first) = ast.items.first() {
            let root = Self::stmt_node(first);
            if let Some(root) = root.parent().or(Some(root)) {
                c.build_semantic(&Self::find_root(root));
            }
        }

        c
    }

    pub(super) fn find_root(node: Node) -> Node {
        let mut n = node;
        while let Some(p) = n.parent() {
            n = p;
        }
        n
    }

    pub(super) fn stmt_node<'a>(stmt: &'a Statement) -> Node<'a> {
        use crate::lng::jass::ast::Statement::*;
        match stmt {
            Type(t) => t.node,
            Native(n) => n.node,
            Function(f) => f.node,
            Globals(g) => g.node,
            Local(l) => l.node,
            Set(s) => s.node,
            Call(c) => c.node,
            Return(r) => r.node,
            Exitwhen(e) => e.node,
            If(i) => i.node,
            Loop(l) => l.node,
            VarStmt(v) => v.node,
            Comment(c) => c.node,
            Import(i) => i.node,
            SetDir(s) => s.node,
            IgnoreDir(ig) => ig.node,
            UjapiImport(u) => u.node,
            EntryDir(e) => e.node,
            Error(e) => e.node,
        }
    }
}
