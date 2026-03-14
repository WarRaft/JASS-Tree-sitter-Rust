use std::collections::{HashMap, HashSet};

use crate::lng::jass::ast::*;
use crate::lng::jass::kind::Kind;
use crate::lng::jass::symbol::{
    FileSymbols, FunctionSym, GlobalVarSym, NativeSym, ParamSym, TypeSym,
};
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity};
use crate::lsp::document_symbol::lsp::{DocumentSymbol, SymbolKind};
use crate::lsp::folding::lsp::{FoldingRange, FoldingRangeKind};
use crate::lsp::highlight::lsp::DocumentHighlightKind;
use crate::lsp::range::Range;
use crate::lsp::ref_map::{DeclKey, ExternalDecl, RawOccurrence, EXTERNAL_KEY_BASE};
use crate::lsp::semantic::hub::Hub;
use crate::lsp::semantic::lsp::Kind as TokenKind;
use crate::util::roper::node::NodeExt;
use lapce_xi_rope::Rope;
use tree_sitter::Node;
use url::Url;

// ─── Imported symbol descriptor ──────────────────────────────────────────────

/// Whether an imported symbol is a function/native or a variable/type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportedKind {
    /// `function` or `native` — resolves in the function namespace.
    Func,
    /// Global variable, constant, or type — resolves in the variable namespace.
    Var,
}

/// A symbol from an imported file that should be visible in the current file.
#[derive(Debug, Clone)]
pub struct ImportedSymbol {
    /// URI of the file that declares this symbol.
    pub origin_uri: Url,
    /// Symbol name.
    pub name: String,
    /// Namespace — function or variable.
    pub kind: ImportedKind,
    /// DeclKey of this symbol in the origin file's RefMap (if known).
    pub origin_decl_key: Option<usize>,
}

// ─── Scope types ─────────────────────────────────────────────────────────────

/// An unresolved reference collected during Phase 1 (local resolution).
/// Will be matched against imported symbols in Phase 2.
#[derive(Debug, Clone)]
struct UnresolvedRef {
    name: String,
    node_start_byte: usize,
    range: Range,
    kind: DocumentHighlightKind,
    /// Which namespace the reference lives in.
    namespace: ImportedKind,
}

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
    pub file_symbols: FileSymbols,

    /// DeclKey → all raw occurrences (declaration + references).
    /// Fed into `build_ref_map()` after the walk.
    pub ref_groups: HashMap<DeclKey, Vec<RawOccurrence>>,
    /// DeclKey → symbol name.
    pub ref_names: HashMap<DeclKey, String>,
    /// Synthetic DeclKey → external declaration (for cross-file definition).
    pub external_decls: HashMap<DeclKey, ExternalDecl>,

    /// DeclKeys that belong to function / native declarations (not variables).
    /// Used by call-graph diagnostics to avoid tagging same-named variables.
    pub func_decl_keys: HashSet<DeclKey>,

    /// Per-file settings parsed from `//set key value` directives.
    pub file_settings: HashMap<String, String>,

    // Working state
    rope: Rope,
    id_roles: HashMap<usize, IdRole>,
    /// Start-bytes of directive nodes (//import, //set) — skipped during CST DFS.
    directive_nodes: HashSet<usize>,
    comment_start: Option<usize>,
    comment_end: usize,
    /// Monotonically increasing counter for declaration ordering.
    decl_counter: usize,
    /// Callee names collected while visiting the current function body.
    /// `None` when outside a function.
    current_callees: Option<HashSet<String>>,
    /// Callee names from bare top-level statements (outside any function).
    pub bare_callees: HashSet<String>,
    /// Name resolution stack for variables and functions (separate namespaces).
    /// Last entry = innermost scope.
    hl_scopes: Vec<HlScope>,
    /// Unresolved references collected during Phase 1.
    /// Linked to imports in Phase 2 via `link_imports()`.
    unresolved_refs: Vec<UnresolvedRef>,
}

/// Two-namespace scope: JASS separates variables and functions by name.
/// `real A = 33` and `function A` can coexist — `A` in expression context
/// resolves to the variable, `A()` in call context resolves to the function.
#[derive(Debug, Clone, Default)]
struct HlScope {
    vars: HashMap<String, DeclKey>,
    funcs: HashMap<String, DeclKey>,
}


impl Cursor {
    /// Walk the AST in two phases, collecting everything.
    ///
    /// **Phase 1** — local-only resolution: the AST is walked with only
    /// file-local declarations in scope.  References that cannot resolve
    /// locally are collected in `unresolved_refs`.
    ///
    /// **Phase 2** — import linking: unresolved refs are matched against
    /// `imported` symbols.  Matched refs get synthetic `DeclKey` values
    /// (≥ `EXTERNAL_KEY_BASE`); unmatched refs become standalone groups
    /// keyed by their own `start_byte`.
    pub fn walk(ast: &Ast, rope: &Rope, imported: &[ImportedSymbol]) -> Self {
        let mut c = Self {
            diagnostics: Vec::new(),
            symbols: Vec::new(),
            folding: Vec::new(),
            semantic: Hub::default(),
            scopes: Vec::new(),
            file_symbols: FileSymbols::new(),
            ref_groups: HashMap::new(),
            ref_names: HashMap::new(),
            external_decls: HashMap::new(),
            func_decl_keys: HashSet::new(),
            file_settings: HashMap::new(),
            rope: rope.clone(),
            id_roles: HashMap::new(),
            directive_nodes: HashSet::new(),
            comment_start: None,
            comment_end: 0,
            decl_counter: 0,
            current_callees: None,
            bare_callees: HashSet::new(),
            hl_scopes: vec![HlScope::default()], // global scope
            unresolved_refs: Vec::new(),
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

        // Phase 1: Walk AST with only local scopes
        c.symbols = c.visit_stmts(&ast.items, &mut Vec::new());

        // Phase 2: Link unresolved refs against imported symbols
        c.link_imports(imported);

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
            Statement::Import(i) => i.node,
            Statement::SetDir(s) => s.node,
            Statement::Error(e) => e.node,
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
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "<unnamed>".into())
    }

    fn id_sel_range(&self, id: &Option<Id>, fallback: &Node) -> Range {
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

    // ─── symbol collection helpers ───────────────────────────────────────

    fn next_decl_index(&mut self) -> usize {
        let idx = self.decl_counter;
        self.decl_counter += 1;
        idx
    }

    /// **Phase 2**: link unresolved references against local forward
    /// declarations, imported symbols, or standalone groups.
    ///
    /// For each unresolved name+namespace pair:
    /// 1. If a **local** declaration exists in the global scope (forward
    ///    reference — e.g. calling a function declared below) → merge into
    ///    the existing local group.
    /// 2. Else if an imported symbol matches → create an external ref group
    ///    (shared `DeclKey ≥ EXTERNAL_KEY_BASE`).
    /// 3. Otherwise → create a **single** standalone local group per
    ///    (name, namespace) pair, keyed by the first occurrence's
    ///    `start_byte`.
    fn link_imports(&mut self, imported: &[ImportedSymbol]) {
        use std::collections::HashMap as Map;

        // Build lookup: (name, namespace) → first ImportedSymbol
        let mut import_lookup: Map<(&str, ImportedKind), &ImportedSymbol> = Map::new();
        for sym in imported {
            import_lookup.entry((sym.name.as_str(), sym.kind)).or_insert(sym);
        }

        // Group unresolved refs by (name, namespace)
        let unresolved = std::mem::take(&mut self.unresolved_refs);
        let mut by_name: Map<(String, ImportedKind), Vec<UnresolvedRef>> = Map::new();
        for uref in unresolved {
            by_name
                .entry((uref.name.clone(), uref.namespace))
                .or_default()
                .push(uref);
        }

        let mut ext_counter: usize = 0;

        for ((name, ns), refs) in by_name {
            // 1. Check local forward declarations (global scope).
            let local_key = if let Some(scope) = self.hl_scopes.first() {
                match ns {
                    ImportedKind::Func => scope.funcs.get(name.as_str()).copied(),
                    ImportedKind::Var  => scope.vars.get(name.as_str()).copied(),
                }
            } else {
                None
            };

            if let Some(key) = local_key {
                // Forward reference → merge into existing local group
                for uref in refs {
                    self.ref_groups
                        .entry(key)
                        .or_default()
                        .push(RawOccurrence {
                            range: uref.range,
                            kind: uref.kind,
                            is_decl: false,
                        });
                }
            } else if let Some(sym) = import_lookup.get(&(name.as_str(), ns)) {
                // 2. Matched an import → external group
                let key = EXTERNAL_KEY_BASE + ext_counter;
                ext_counter += 1;
                self.ref_names.insert(key, name.clone());
                self.external_decls.insert(
                    key,
                    ExternalDecl {
                        uri: sym.origin_uri.clone(),
                        name: name.clone(),
                        origin_decl_key: sym.origin_decl_key,
                    },
                );
                for uref in refs {
                    self.ref_groups
                        .entry(key)
                        .or_default()
                        .push(RawOccurrence {
                            range: uref.range,
                            kind: uref.kind,
                            is_decl: false,
                        });
                }
            } else {
                // 3. No match → single standalone group per (name, ns).
                let key = refs[0].node_start_byte;
                self.ref_names
                    .entry(key)
                    .or_insert_with(|| name.clone());
                for (i, uref) in refs.iter().enumerate() {
                    self.ref_groups
                        .entry(key)
                        .or_default()
                        .push(RawOccurrence {
                            range: uref.range.clone(),
                            kind: uref.kind,
                            is_decl: i == 0, // first = "declaration"
                        });
                }
            }
        }
    }

    fn params_to_sym(&self, params: &[Param]) -> Vec<ParamSym> {
        params
            .iter()
            .map(|p| ParamSym {
                name: self.id_name(&p.name),
                type_name: self.id_name(&p.type_id),
            })
            .collect()
    }

    /// Record a callee name.  If we're inside a function body it goes into
    /// `current_callees`; otherwise it's a bare top-level call → `bare_callees`.
    fn record_callee(&mut self, name: &str) {
        if let Some(ref mut callees) = self.current_callees {
            callees.insert(name.to_string());
        } else {
            self.bare_callees.insert(name.to_string());
        }
    }

    // ─── ref helpers (highlight / definition / references / rename) ────

    /// Declare a **variable** (global, local, param, constant).
    fn hl_declare_var(&mut self, name: &str, node: &Node) -> DeclKey {
        let key = node.start_byte();
        let range = node.to_range(&self.rope);

        self.ref_groups
            .entry(key)
            .or_default()
            .push(RawOccurrence {
                range,
                kind: DocumentHighlightKind::Write,
                is_decl: true,
            });
        self.ref_names.insert(key, name.to_string());

        if let Some(scope) = self.hl_scopes.last_mut() {
            scope.vars.insert(name.to_string(), key);
        }
        key
    }

    /// Declare a **function / native**.
    fn hl_declare_func(&mut self, name: &str, node: &Node) -> DeclKey {
        let key = node.start_byte();
        let range = node.to_range(&self.rope);

        self.ref_groups
            .entry(key)
            .or_default()
            .push(RawOccurrence {
                range,
                kind: DocumentHighlightKind::Write,
                is_decl: true,
            });
        self.ref_names.insert(key, name.to_string());
        self.func_decl_keys.insert(key);

        if let Some(scope) = self.hl_scopes.last_mut() {
            scope.funcs.insert(name.to_string(), key);
        }
        key
    }

    /// Declare a **type**.  Types share the variable namespace for simplicity.
    fn hl_declare_type(&mut self, name: &str, node: &Node) -> DeclKey {
        self.hl_declare_var(name, node)
    }

    /// Record a reference to a previously-declared **variable**.
    /// If the name is not found in any local scope, the reference is
    /// collected in `unresolved_refs` for Phase 2 import linking.
    fn hl_reference_var(&mut self, name: &str, node: &Node, kind: DocumentHighlightKind) {
        let decl_key = self
            .hl_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.vars.get(name).copied());

        if let Some(key) = decl_key {
            let range = node.to_range(&self.rope);
            self.ref_groups
                .entry(key)
                .or_default()
                .push(RawOccurrence {
                    range,
                    kind,
                    is_decl: false,
                });
            self.ref_names.entry(key).or_insert_with(|| name.to_string());
        } else {
            self.unresolved_refs.push(UnresolvedRef {
                name: name.to_string(),
                node_start_byte: node.start_byte(),
                range: node.to_range(&self.rope),
                kind,
                namespace: ImportedKind::Var,
            });
        }
    }

    /// JASS built-in primitive types that have no user declaration.
    /// These are silently skipped by `hl_reference_type` because they
    /// cannot be imported or renamed.
    const PRIMITIVE_TYPES: &'static [&'static str] = &[
        "integer", "real", "boolean", "string", "handle", "code", "nothing",
    ];

    /// Record a reference to a **type** name (shares the variable namespace).
    ///
    /// If the type is found locally, it is linked to the local declaration.
    /// If not, it is pushed to `unresolved_refs` for Phase 2 import linking
    /// — except for JASS primitive types (`integer`, `real`, `handle`, …)
    /// which have no user declaration and are silently skipped.
    fn hl_reference_type(&mut self, name: &str, node: &Node, kind: DocumentHighlightKind) {
        let decl_key = self
            .hl_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.vars.get(name).copied());

        if let Some(key) = decl_key {
            let range = node.to_range(&self.rope);
            self.ref_groups
                .entry(key)
                .or_default()
                .push(RawOccurrence {
                    range,
                    kind,
                    is_decl: false,
                });
            self.ref_names.entry(key).or_insert_with(|| name.to_string());
        } else if !Self::PRIMITIVE_TYPES.contains(&name) {
            // Not a built-in primitive → push for import matching
            self.unresolved_refs.push(UnresolvedRef {
                name: name.to_string(),
                node_start_byte: node.start_byte(),
                range: node.to_range(&self.rope),
                kind,
                namespace: ImportedKind::Var,
            });
        }
    }

    /// Record a reference to a previously-declared **function / native**.
    /// If the name is not found in any local scope, the reference is
    /// collected in `unresolved_refs` for Phase 2 import linking.
    fn hl_reference_func(&mut self, name: &str, node: &Node, kind: DocumentHighlightKind) {
        let decl_key = self
            .hl_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.funcs.get(name).copied());

        if let Some(key) = decl_key {
            let range = node.to_range(&self.rope);
            self.ref_groups
                .entry(key)
                .or_default()
                .push(RawOccurrence {
                    range,
                    kind,
                    is_decl: false,
                });
            self.ref_names.entry(key).or_insert_with(|| name.to_string());
        } else {
            self.unresolved_refs.push(UnresolvedRef {
                name: name.to_string(),
                node_start_byte: node.start_byte(),
                range: node.to_range(&self.rope),
                kind,
                namespace: ImportedKind::Func,
            });
        }
    }

    /// Push a new highlight scope (e.g. entering a function body).
    fn hl_push_scope(&mut self) {
        self.hl_scopes.push(HlScope::default());
    }

    /// Pop the innermost highlight scope (e.g. leaving a function body).
    fn hl_pop_scope(&mut self) {
        self.hl_scopes.pop();
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
        // Import directives — skip comment tracking, add dedicated semantic
        if let Statement::Import(imp) = stmt {
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
        if let Statement::SetDir(sd) = stmt {
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
                let name = self.id_name(&t.name);
                let decl_index = self.next_decl_index();
                if let Some(ref name_id) = t.name {
                    self.hl_declare_type(&name, &name_id.node);
                }
                if let Some(ref base_id) = t.base {
                    let bname = self.node_text(&base_id.node);
                    self.hl_reference_type(&bname, &base_id.node, DocumentHighlightKind::Read);
                }
                self.file_symbols.types.push(TypeSym {
                    name: name.clone(),
                    base: t.base.as_ref().map(|id| self.node_text(&id.node)),
                    decl_index,
                });
                Some(DocumentSymbol {
                    name,
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
                let name = self.id_name(&n.name);
                let decl_index = self.next_decl_index();
                if let Some(ref name_id) = n.name {
                    self.hl_declare_func(&name, &name_id.node);
                }
                // hl: reference parameter types
                for p in &n.params {
                    if let Some(ref tid) = p.type_id {
                        let tname = self.node_text(&tid.node);
                        self.hl_reference_type(&tname, &tid.node, DocumentHighlightKind::Read);
                    }
                }
                // hl: reference return type
                if let Some(ref rt_id) = n.return_type {
                    let rt_name = self.node_text(&rt_id.node);
                    self.hl_reference_type(&rt_name, &rt_id.node, DocumentHighlightKind::Read);
                }
                self.file_symbols.natives.push(NativeSym {
                    name: name.clone(),
                    params: self.params_to_sym(&n.params),
                    return_type: n.return_type.as_ref().map(|id| self.node_text(&id.node)),
                    decl_index,
                });
                Some(DocumentSymbol {
                    name,
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

                let func_name = self.id_name(&f.name);
                let decl_index = self.next_decl_index();
                let param_syms = self.params_to_sym(&f.params);
                let return_type = f.return_type.as_ref().map(|id| self.node_text(&id.node));

                // hl: declare function name in the current (global) scope
                if let Some(ref name_id) = f.name {
                    self.hl_declare_func(&func_name, &name_id.node);
                }
                // hl: reference return type
                if let Some(ref rt_id) = f.return_type {
                    let rt_name = self.node_text(&rt_id.node);
                    self.hl_reference_type(&rt_name, &rt_id.node, DocumentHighlightKind::Read);
                }

                // Start collecting callees for this function.
                self.current_callees = Some(HashSet::new());

                // hl: push function scope for params and locals
                self.hl_push_scope();

                let mut func_vars = HashMap::new();
                let mut children = Vec::new();

                for p in &f.params {
                    self.register_id(&p.type_id);
                    self.register_id(&p.name);
                    if let Some(name_id) = &p.name {
                        let pname = self.node_text(&name_id.node);
                        let type_name = p.type_id.as_ref().map(|id| self.node_text(&id.node));
                        // hl: reference param type
                        if let Some(ref tid) = p.type_id {
                            let tname = self.node_text(&tid.node);
                            self.hl_reference_type(&tname, &tid.node, DocumentHighlightKind::Read);
                        }
                        // hl: declare param
                        self.hl_declare_var(&pname, &name_id.node);
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

                // hl: pop function scope
                self.hl_pop_scope();

                // Finalize callee collection.
                let callees = self.current_callees.take().unwrap_or_default();

                self.file_symbols.functions.push(FunctionSym {
                    name: func_name.clone(),
                    params: param_syms,
                    return_type,
                    decl_index,
                    callees,
                });

                self.scopes.push(Scope {
                    name: func_name.clone(),
                    vars: func_vars,
                });

                Some(DocumentSymbol {
                    name: func_name,
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
                    // hl: reference the type
                    if let Some(ref tid) = v.type_id {
                        let tname = self.node_text(&tid.node);
                        self.hl_reference_type(&tname, &tid.node, DocumentHighlightKind::Read);
                    }

                    for d in &v.decls {
                        self.register_id(&d.name);
                        if let Some(expr) = &d.value {
                            self.visit_expr(expr);
                        }
                        let var_name = self.id_name(&d.name);
                        let decl_index = self.next_decl_index();
                        if let Some(name_id) = &d.name {
                            // hl: declare global variable
                            self.hl_declare_var(&var_name, &name_id.node);
                            Self::scope_define(
                                vars.last_mut().unwrap(),
                                &self.node_text(&name_id.node),
                                name_id.node.start_byte(),
                                type_name.clone(),
                                v.is_array, v.is_constant, d.value.is_some(),
                            );
                        }
                        self.file_symbols.globals.push(GlobalVarSym {
                            name: var_name.clone(),
                            type_name: type_name.clone(),
                            is_constant: v.is_constant,
                            is_array: v.is_array,
                            has_initializer: d.value.is_some(),
                            decl_index,
                        });
                        children.push(DocumentSymbol {
                            name: var_name,
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
                // hl: reference the type
                if let Some(ref tid) = l.type_id {
                    let tname = self.node_text(&tid.node);
                    self.hl_reference_type(&tname, &tid.node, DocumentHighlightKind::Read);
                }
                if let Some(expr) = &l.value {
                    self.visit_expr(expr);
                }
                if let (Some(scope), Some(name_id)) = (vars.last_mut(), &l.name) {
                    let lname = self.node_text(&name_id.node);
                    // hl: declare local variable
                    self.hl_declare_var(&lname, &name_id.node);
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
                // hl: reference the type
                if let Some(ref tid) = v.type_id {
                    let tname = self.node_text(&tid.node);
                    self.hl_reference_type(&tname, &tid.node, DocumentHighlightKind::Read);
                }
                for d in &v.decls {
                    self.register_id(&d.name);
                    if let Some(ref name_id) = d.name {
                        let vname = self.node_text(&name_id.node);
                        // hl: declare variable
                        self.hl_declare_var(&vname, &name_id.node);
                    }
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
                    // hl: reference variable as Write
                    self.hl_reference_var(&name, &var_id.node, DocumentHighlightKind::Write);
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
                    if let Some(name_id) = &fc.name {
                        let fname = self.node_text(&name_id.node);
                        self.record_callee(&fname);
                        // hl: reference function as Read
                        self.hl_reference_func(&fname, &name_id.node, DocumentHighlightKind::Read);
                    }
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
            Statement::Import(_) => unreachable!("handled above"),
            Statement::SetDir(_) => unreachable!("handled above"),
            Statement::Error(_) => None, // diagnostics already collected from ast.errors
        }
    }

    // ─── Expression visitor ────────────────────────────────────────────

    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Id(id) => {
                self.id_roles.insert(id.node.start_byte(), id.role);
                let name = self.node_text(&id.node);
                self.hl_reference_var(&name, &id.node, DocumentHighlightKind::Read);
            }
            Expr::Call(fc) => {
                self.register_id(&fc.name);
                if let Some(name_id) = &fc.name {
                    let fname = self.node_text(&name_id.node);
                    self.record_callee(&fname);
                    self.hl_reference_func(&fname, &name_id.node, DocumentHighlightKind::Read);
                }
                for arg in &fc.args {
                    self.visit_expr(arg);
                }
            }
            Expr::FuncRef(id) => {
                self.id_roles.insert(id.node.start_byte(), id.role);
                let fname = self.node_text(&id.node);
                self.record_callee(&fname);
                self.hl_reference_func(&fname, &id.node, DocumentHighlightKind::Read);
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

                // Import directive comment nodes are handled in the AST pass —
                // skip them entirely so they don't get re-coloured as Comment.
                if kind == Some(Kind::Comment) && self.directive_nodes.contains(&node.start_byte()) {
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    while !cursor.goto_next_sibling() {
                        if !cursor.goto_parent() {
                            return;
                        }
                    }
                    continue;
                }

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


