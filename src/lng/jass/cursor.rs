use std::collections::{HashMap, HashSet};

use crate::lng::jass::ast::*;
use crate::lng::jass::kind::Kind;
use crate::lng::jass::symbol::{
    FileSymbols, FunctionSym, GlobalVarSym, NativeSym, ParamSym, TypeSym,
};
use crate::lng::jass::type_map::{
    ComptimeValue, DeclType, FuncType, ParamPair, TypeDeclInfo, TypeMap, VarType, UNKNOWN_TYPE,
};
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity};
use crate::lsp::document_symbol::lsp::{DocumentSymbol, SymbolKind};
use crate::lsp::folding::lsp::{FoldingRange, FoldingRangeKind};
use crate::lsp::highlight::lsp::DocumentHighlightKind;
use crate::lsp::inlay_hint::lsp::{InlayHint, InlayHintKind};
use crate::lsp::position::Position;
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
    pub origin_decl_key: Option<DeclKey>,
    /// Return type for functions/natives (e.g. `"unit"`); `None` for variables.
    pub return_type: Option<String>,
    /// Type name for variables (e.g. `"integer"`); `None` for functions.
    pub type_name: Option<String>,
}

// ─── Scope types ─────────────────────────────────────────────────────────────

/// An unresolved reference collected during Phase 1 (local resolution).
/// Will be matched against imported symbols in Phase 2.
#[derive(Debug, Clone)]
struct UnresolvedRef {
    name: String,
    range: Range,
    kind: DocumentHighlightKind,
    /// Which namespace the reference lives in.
    namespace: ImportedKind,
    /// `true` when this reference comes from a **type** position
    /// (e.g. `local MyType x`).  Used only for the diagnostic label:
    /// "Undeclared type" vs "Undeclared variable".
    is_type_ref: bool,
}

/// A handle-type local variable that needs leak checking.
#[allow(dead_code)]
struct HandleLocal {
    name: String,
    type_name: String,
    range: Range,
    has_value: bool,
}

// ─── Flow-sensitive nullability ──────────────────────────────────────────────

/// Three-value nullability lattice for handle leak analysis.
///
/// Tracks whether a handle-type local variable is definitely null,
/// definitely non-null, or could be either at a given program point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NullState {
    /// Definitely `null` (e.g. uninitialized or `set x = null`).
    Null,
    /// Definitely not `null` (e.g. `set x = CreateUnit()`).
    NonNull,
    /// Could be either — divergent branches merged.
    MaybeNull,
}

impl NullState {
    /// Lattice merge: same→same, different→MaybeNull.
    fn join(a: NullState, b: NullState) -> NullState {
        if a == b { a } else { NullState::MaybeNull }
    }
}

/// Per-variable null state map used during flow analysis.
type NullMap = HashMap<String, NullState>;

/// Result of inspecting an if-condition for `var == null` / `var != null`.
struct NullGuard {
    /// The variable name mentioned in the null check.
    var_name: String,
    /// `true`  → condition is `var != null` (then-branch implies non-null).
    /// `false` → condition is `var == null` (then-branch implies null).
    is_neq: bool,
}


/// Info about a variable inside a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarInfo {
    pub start_byte: usize,
    pub type_name: Option<String>,
    pub is_array: bool,
    pub is_constant: bool,
    pub is_initialized: bool,
    pub is_param: bool,
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
    /// File-level diagnostic suppression tags from `//ignore tag` directives.
    pub file_ignore_tags: HashSet<String>,

    /// Per-declaration resolved types.
    pub type_map: TypeMap,
    /// Inlay hints for type annotations (shown when `//set type-tip 1`).
    pub type_hints: Vec<InlayHint>,
    /// Compile-time evaluated values of `constant` globals.
    /// Keyed by variable name; used by `eval_expr` to propagate values
    /// and by type hints to display `type(value)`.
    comptime_values: HashMap<String, ComptimeValue>,

    // Working state
    rope: Rope,
    id_roles: HashMap<usize, IdRole>,
    /// Start-bytes of directive nodes (//import, //set) — skipped during CST DFS.
    directive_nodes: HashSet<usize>,
    comment_start: Option<usize>,
    comment_end: usize,
    /// Monotonically increasing counter for declaration ordering.
    decl_counter: usize,
    /// Next ordinal DeclKey to assign (0, 1, 2, …).
    next_decl_key: DeclKey,
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
    /// Imported function return types: name → return_type.
    /// Populated from `ImportedSymbol` at the start of `walk`.
    imported_func_returns: HashMap<String, Option<String>>,
    /// Imported variable types: name → type_name.
    imported_var_types: HashMap<String, Option<String>>,
}

/// Two-namespace scope: JASS separates variables and functions by name.
/// `real A = 33` and `function A` can coexist — `A` in expression context
/// resolves to the variable, `A()` in call context resolves to the function.
#[derive(Debug, Clone, Default)]
struct HlScope {
    vars: HashMap<String, DeclKey>,
    funcs: HashMap<String, DeclKey>,
}

/// Strip the `//*` prefix from a single comment line and return the doc text.
///
/// Rules:
/// - `//* foo` → `foo`   (strip `//*` + one trailing space)
/// - `//*foo`  → `foo`   (strip `//*` only, no space to remove)
/// - `//*`     → ``      (empty line)
fn strip_doc_prefix(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("//*") {
        return None;
    }
    let after = &trimmed[3..];
    if after.starts_with(' ') {
        Some(&after[1..])
    } else {
        Some(after)
    }
}

/// Strip the `//@ignore` prefix and return the list of tags.
///
/// Rules:
/// - `//@ignore unused cycle` → `["unused", "cycle"]`
/// - `//@ignore unused`       → `["unused"]`
/// - `//@ignore`              → `[]` (no tags)
fn strip_ignore_prefix(line: &str) -> Option<Vec<&str>> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("//@ignore") {
        return None;
    }
    let after = &trimmed["//@ignore".len()..];
    if after.is_empty() || after.starts_with(' ') || after.starts_with('\t') {
        let tags: Vec<&str> = after.split_whitespace().collect();
        Some(tags)
    } else {
        None
    }
}

/// Annotations extracted from the comment block above a declaration.
struct CommentAnnotations {
    doc_comment: Option<String>,
    ignore_tags: HashSet<String>,
}

/// Extract annotations (`//*` doc comment and `//@ignore` tags) from the
/// comment block directly above a declaration at `row`.
///
/// Walks upward from `row - 1` collecting consecutive lines that start with
/// `//*` or `//@ignore`.  Stops at the first line that matches neither.
fn extract_annotations(rope: &Rope, row: usize) -> CommentAnnotations {
    let mut doc_lines = Vec::new();
    let mut ignore_tags = HashSet::new();

    if row == 0 {
        return CommentAnnotations { doc_comment: None, ignore_tags };
    }
    let line_count = rope.line_of_offset(rope.len()) + 1;
    let mut r = row;
    while r > 0 {
        r -= 1;
        if r >= line_count {
            break;
        }
        let line_start = rope.offset_of_line(r);
        let line_end = if r + 1 < line_count {
            rope.offset_of_line(r + 1)
        } else {
            rope.len()
        };
        let text = rope.slice_to_cow(line_start..line_end);
        let text = text.trim_end_matches('\n').trim_end_matches('\r');
        if let Some(doc) = strip_doc_prefix(text) {
            doc_lines.push(doc.to_string());
        } else if let Some(tags) = strip_ignore_prefix(text) {
            for tag in tags {
                ignore_tags.insert(tag.to_string());
            }
        } else {
            break;
        }
    }

    let doc_comment = if doc_lines.is_empty() {
        None
    } else {
        doc_lines.reverse();
        Some(doc_lines.join("\n"))
    };

    CommentAnnotations { doc_comment, ignore_tags }
}



impl Cursor {
    /// Walk the AST in two phases, collecting everything the LSP needs.
    ///
    /// ## Phase 1 — local resolution (single top-to-bottom walk)
    ///
    /// Every AST node is visited once.  Declarations (globals, locals,
    /// functions, natives, types, params) create [`DeclKey`] entries in
    /// `hl_scopes` and `ref_groups`.  References that resolve within the
    /// current file's scopes are linked immediately.  References that
    /// **cannot** resolve locally (e.g. `B` used before `boolean B` is
    /// declared, or a function from an imported file) are collected into
    /// `unresolved_refs`.
    ///
    /// Phase 1 also computes: type inference (`visit_expr`), semantic
    /// tokens, document symbols, folding ranges, inlay hints, and
    /// `FileSymbols` (the file's export list).
    ///
    /// ## Phase 2 — import linking ([`link_imports`])
    ///
    /// For each `(name, namespace)` pair in `unresolved_refs`:
    ///
    /// 1. **Forward local declaration** — if a declaration for `name` now
    ///    exists in the global scope (it was declared below the reference
    ///    in Phase 1), merge into the existing group.  This covers
    ///    `B = 3` before `boolean B` within the same file.
    ///
    /// 2. **Imported symbol** — if `name` matches an entry from `imported`
    ///    (another file in the connected component), create an external
    ///    ref group with `DeclKey ≥ EXTERNAL_KEY_BASE`.
    ///
    /// 3. **Truly unresolved** — emit `"Undeclared variable/function"`
    ///    diagnostics and create a standalone group.
    ///
    /// This two-phase design guarantees that:
    /// - Forward references within a file are always resolved.
    /// - Cross-file symbols are resolved when the dependency is available.
    /// - Only truly unknown names produce diagnostics.
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
            file_ignore_tags: HashSet::new(),
            type_map: TypeMap::default(),
            type_hints: Vec::new(),
            comptime_values: HashMap::new(),
            rope: rope.clone(),
            id_roles: HashMap::new(),
            directive_nodes: HashSet::new(),
            comment_start: None,
            comment_end: 0,
            decl_counter: 0,
            next_decl_key: 0,
            current_callees: None,
            bare_callees: HashSet::new(),
            hl_scopes: vec![HlScope::default()], // global scope
            unresolved_refs: Vec::new(),
            imported_func_returns: HashMap::new(),
            imported_var_types: HashMap::new(),
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
            Statement::IgnoreDir(ig) => ig.node,
            Statement::UjapiImport(u) => u.node,
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
        is_param: bool,
    ) {
        vars.insert(
            name.to_string(),
            VarInfo { start_byte, type_name, is_array, is_constant, is_initialized, is_param },
        );
    }

    // ─── symbol collection helpers ───────────────────────────────────────

    fn next_decl_index(&mut self) -> usize {
        let idx = self.decl_counter;
        self.decl_counter += 1;
        idx
    }

    /// Allocate the next sequential `DeclKey` (0, 1, 2, …).
    fn alloc_key(&mut self) -> DeclKey {
        let key = self.next_decl_key;
        self.next_decl_key += 1;
        key
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
    ///    `start_byte`, **and emit "Undeclared" diagnostics**.
    fn link_imports(&mut self, imported: &[ImportedSymbol]) {
        use std::collections::HashMap as Map;
        use crate::lsp::ref_map::ExternalOrigin;

        // Build lookup: (name, namespace) → ALL matching ImportedSymbols
        let mut import_lookup: Map<(&str, ImportedKind), Vec<&ImportedSymbol>> = Map::new();
        for sym in imported {
            import_lookup
                .entry((sym.name.as_str(), sym.kind))
                .or_default()
                .push(sym);
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

        let mut ext_counter: u32 = 0;

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
            } else if let Some(syms) = import_lookup.get(&(name.as_str(), ns)) {
                // 2. Matched imports → external group with ALL origins
                let key = EXTERNAL_KEY_BASE + ext_counter;
                ext_counter += 1;
                self.ref_names.insert(key, name.clone());

                // Deduplicate origins by URI (same file may appear multiple times)
                let mut seen_uris = std::collections::HashSet::new();
                let mut origins = Vec::new();
                for sym in syms {
                    if seen_uris.insert(sym.origin_uri.as_str().to_string()) {
                        origins.push(ExternalOrigin {
                            uri: sym.origin_uri.clone(),
                            origin_decl_key: sym.origin_decl_key,
                        });
                    }
                }

                self.external_decls.insert(
                    key,
                    ExternalDecl {
                        name: name.clone(),
                        origins,
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
                //    Emit "Undeclared type/variable/function" diagnostics for
                //    each occurrence.
                let key = self.alloc_key();
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
                    let label = if uref.is_type_ref {
                        "type"
                    } else {
                        match ns {
                            ImportedKind::Func => "function",
                            ImportedKind::Var  => "variable",
                        }
                    };
                    self.diagnostics.push(Diagnostic {
                        range: uref.range.clone(),
                        message: format!("Undeclared {} `{}`", label, name),
                        severity: Some(DiagnosticSeverity::Error),
                        ..Default::default()
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
        let key = self.alloc_key();
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
        let key = self.alloc_key();
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

    /// JASS built-in value literals that are not user-declared variables.
    /// These are silently skipped by `hl_reference_var` because they
    /// cannot be imported or renamed.
    const BUILTIN_VALUES: &'static [&'static str] = &["true", "false", "null"];

    /// Record a reference to a previously-declared **variable**.
    /// If the name is not found in any local scope, the reference is
    /// collected in `unresolved_refs` for Phase 2 import linking.
    /// Built-in value literals (`true`, `false`, `null`) are silently
    /// skipped — they have no user declaration.
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
        } else if !Self::BUILTIN_VALUES.contains(&name) {
            self.unresolved_refs.push(UnresolvedRef {
                name: name.to_string(),
                range: node.to_range(&self.rope),
                kind,
                namespace: ImportedKind::Var,
                is_type_ref: false,
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
                range: node.to_range(&self.rope),
                kind,
                namespace: ImportedKind::Var,
                is_type_ref: true,
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
                range: node.to_range(&self.rope),
                kind,
                namespace: ImportedKind::Func,
                is_type_ref: false,
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

    // ─── type system helpers ─────────────────────────────────────────────

    /// Emit an `InlayHint` right after `node` showing `label` as a type tag.
    ///
    /// When `value` is `Some`, the hint is formatted as `: type(value)`.
    fn emit_type_hint(&mut self, node: &Node, label: &str, value: Option<&ComptimeValue>) {
        let end = node.end_position();
        let display = match value {
            Some(v) => format!(": {}({})", label, v),
            None => format!(": {}", label),
        };
        self.type_hints.push(InlayHint {
            position: Position {
                line: end.row,
                character: end.column,
            },
            label: display,
            kind: Some(InlayHintKind::Type),
            padding_left: Some(true),
            padding_right: Some(false),
        });
    }

    /// Format a human-readable type label with optional modifiers.
    ///
    /// Examples: `integer`, `constant real`, `comptime integer`, `integer array`.
    fn build_type_label(
        type_name: &str,
        is_constant: bool,
        is_comptime: bool,
        is_array: bool,
    ) -> String {
        let mut parts = Vec::new();
        if is_comptime {
            parts.push("comptime");
        } else if is_constant {
            parts.push("constant");
        }
        parts.push(type_name);
        if is_array {
            parts.push("array");
        }
        parts.join(" ")
    }

    /// Evaluate an expression at compile time, returning the computed value
    /// if the expression consists exclusively of literals, `comptime` globals,
    /// and pure operators.
    fn eval_expr(&self, expr: &Expr) -> Option<ComptimeValue> {
        match expr {
            Expr::Literal(node) => self.eval_literal(node),
            Expr::Id(id) => {
                let name = self.node_text(&id.node);
                match name.as_str() {
                    "true" => Some(ComptimeValue::Bool(true)),
                    "false" => Some(ComptimeValue::Bool(false)),
                    "null" => Some(ComptimeValue::Null),
                    _ => self.comptime_values.get(&name).cloned(),
                }
            }
            Expr::Binary { node, left, right } => {
                let lv = self.eval_expr(left)?;
                let rv = self.eval_expr(right)?;
                let op = Self::binary_op_kind(node)?;
                Self::eval_binary_comptime(op, &lv, &rv)
            }
            Expr::Unary { node, operand } => {
                let v = self.eval_expr(operand)?;
                let op = Self::unary_op_kind(node)?;
                Self::eval_unary_comptime(op, &v)
            }
            Expr::Parens { inner, .. } => self.eval_expr(inner),
            // Function calls, array accesses, and function references
            // are never comptime in JASS.
            Expr::Call(_) | Expr::FuncRef(_) | Expr::Index { .. } => None,
        }
    }

    /// Evaluate a literal CST node at compile time.
    fn eval_literal(&self, node: &Node) -> Option<ComptimeValue> {
        let kind = Kind::try_from(node.kind_id()).ok()?;
        match kind {
            Kind::Number => {
                let text = self.node_text(node);
                Self::parse_integer_literal(&text).map(ComptimeValue::Integer)
            }
            Kind::Float => {
                let text = self.node_text(node);
                text.parse::<f64>().ok().map(ComptimeValue::Real)
            }
            Kind::StringLiteral => {
                let text = self.node_text(node);
                let inner = if text.len() >= 2 {
                    &text[1..text.len() - 1]
                } else {
                    ""
                };
                Some(ComptimeValue::Str(Self::unescape_jass_string(inner)))
            }
            Kind::Rawcode => {
                let text = self.node_text(node);
                let inner = if text.len() >= 2 {
                    &text[1..text.len() - 1]
                } else {
                    ""
                };
                let mut val: i64 = 0;
                for b in inner.bytes() {
                    val = (val << 8) | (b as i64);
                }
                Some(ComptimeValue::Integer(val))
            }
            _ => None,
        }
    }

    /// Parse a JASS integer literal (decimal, hex `0x…`, octal `0…`).
    fn parse_integer_literal(text: &str) -> Option<i64> {
        if text.len() > 2 && (text.starts_with("0x") || text.starts_with("0X")) {
            i64::from_str_radix(&text[2..], 16).ok()
        } else if text.starts_with('$') && text.len() > 1 {
            // Alternate hex prefix used in some JASS dialects
            i64::from_str_radix(&text[1..], 16).ok()
        } else if text.starts_with('0') && text.len() > 1 && text.chars().all(|c| c.is_ascii_digit()) {
            i64::from_str_radix(&text[1..], 8).ok()
        } else {
            text.parse::<i64>().ok()
        }
    }

    /// Basic JASS string unescape: `\\` → `\`, `\"` → `"`, `\n` → newline.
    fn unescape_jass_string(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('\\') => out.push('\\'),
                    Some('"') => out.push('"'),
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Evaluate a binary operation on two compile-time values.
    fn eval_binary_comptime(op: Kind, left: &ComptimeValue, right: &ComptimeValue) -> Option<ComptimeValue> {
        use ComptimeValue::*;
        match op {
            Kind::Plus => match (left, right) {
                (Integer(a), Integer(b)) => Some(Integer(a.wrapping_add(*b))),
                (Real(a), Real(b)) => Some(Real(a + b)),
                (Integer(a), Real(b)) => Some(Real(*a as f64 + b)),
                (Real(a), Integer(b)) => Some(Real(a + *b as f64)),
                // String concatenation — JASS converts the other operand.
                (Str(a), Str(b)) => Some(Str(format!("{}{}", a, b))),
                (Str(a), Integer(b)) => Some(Str(format!("{}{}", a, b))),
                (Str(a), Real(b)) => Some(Str(format!("{}{}", a, b))),
                (Integer(a), Str(b)) => Some(Str(format!("{}{}", a, b))),
                (Real(a), Str(b)) => Some(Str(format!("{}{}", a, b))),
                _ => None,
            },
            Kind::Minus => match (left, right) {
                (Integer(a), Integer(b)) => Some(Integer(a.wrapping_sub(*b))),
                (Real(a), Real(b)) => Some(Real(a - b)),
                (Integer(a), Real(b)) => Some(Real(*a as f64 - b)),
                (Real(a), Integer(b)) => Some(Real(a - *b as f64)),
                _ => None,
            },
            Kind::Star => match (left, right) {
                (Integer(a), Integer(b)) => Some(Integer(a.wrapping_mul(*b))),
                (Real(a), Real(b)) => Some(Real(a * b)),
                (Integer(a), Real(b)) => Some(Real(*a as f64 * b)),
                (Real(a), Integer(b)) => Some(Real(a * *b as f64)),
                _ => None,
            },
            Kind::Slash => match (left, right) {
                (Integer(a), Integer(b)) if *b != 0 => Some(Integer(a / b)),
                (Real(a), Real(b)) if *b != 0.0 => Some(Real(a / b)),
                (Integer(a), Real(b)) if *b != 0.0 => Some(Real(*a as f64 / b)),
                (Real(a), Integer(b)) if *b != 0 => Some(Real(a / *b as f64)),
                _ => None,
            },
            Kind::And => match (left, right) {
                (Bool(a), Bool(b)) => Some(Bool(*a && *b)),
                _ => None,
            },
            Kind::Or => match (left, right) {
                (Bool(a), Bool(b)) => Some(Bool(*a || *b)),
                _ => None,
            },
            Kind::EqEq => match (left, right) {
                (Integer(a), Integer(b)) => Some(Bool(a == b)),
                (Real(a), Real(b)) => Some(Bool(a == b)),
                (Str(a), Str(b)) => Some(Bool(a == b)),
                (Bool(a), Bool(b)) => Some(Bool(a == b)),
                _ => None,
            },
            Kind::Neq => match (left, right) {
                (Integer(a), Integer(b)) => Some(Bool(a != b)),
                (Real(a), Real(b)) => Some(Bool(a != b)),
                (Str(a), Str(b)) => Some(Bool(a != b)),
                (Bool(a), Bool(b)) => Some(Bool(a != b)),
                _ => None,
            },
            Kind::Lt => match (left, right) {
                (Integer(a), Integer(b)) => Some(Bool(a < b)),
                (Real(a), Real(b)) => Some(Bool(a < b)),
                (Str(a), Str(b)) => Some(Bool(a < b)),
                _ => None,
            },
            Kind::Gt => match (left, right) {
                (Integer(a), Integer(b)) => Some(Bool(a > b)),
                (Real(a), Real(b)) => Some(Bool(a > b)),
                (Str(a), Str(b)) => Some(Bool(a > b)),
                _ => None,
            },
            Kind::Le => match (left, right) {
                (Integer(a), Integer(b)) => Some(Bool(a <= b)),
                (Real(a), Real(b)) => Some(Bool(a <= b)),
                (Str(a), Str(b)) => Some(Bool(a <= b)),
                _ => None,
            },
            Kind::Ge => match (left, right) {
                (Integer(a), Integer(b)) => Some(Bool(a >= b)),
                (Real(a), Real(b)) => Some(Bool(a >= b)),
                (Str(a), Str(b)) => Some(Bool(a >= b)),
                _ => None,
            },
            _ => None,
        }
    }

    /// Evaluate a unary operation on a compile-time value.
    fn eval_unary_comptime(op: Kind, val: &ComptimeValue) -> Option<ComptimeValue> {
        use ComptimeValue::*;
        match op {
            Kind::Minus => match val {
                Integer(v) => Some(Integer(-v)),
                Real(v) => Some(Real(-v)),
                _ => None,
            },
            Kind::Not => match val {
                Bool(v) => Some(Bool(!v)),
                _ => None,
            },
            _ => None,
        }
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

        // IgnoreDir directives — skip comment tracking, add dedicated semantic
        if let Statement::IgnoreDir(ig) = stmt {
            self.flush_comment_run();
            self.directive_nodes.insert(ig.node.start_byte());
            crate::lng::directive::visit_ignore_semantic(
                ig,
                &mut self.semantic,
                &mut self.diagnostics,
                &mut self.file_ignore_tags,
                &self.rope,
            );
            return None;
        }

        // UjapiImport directives — skip comment tracking, add dedicated semantic
        if let Statement::UjapiImport(ud) = stmt {
            self.flush_comment_run();
            self.directive_nodes.insert(ud.node.start_byte());
            crate::lng::directive::visit_ujapi_semantic(
                ud,
                &mut self.semantic,
                &mut self.diagnostics,
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
                let decl_key = if let Some(ref name_id) = t.name {
                    Some(self.hl_declare_type(&name, &name_id.node))
                } else {
                    None
                };
                if let Some(ref base_id) = t.base {
                    let bname = self.node_text(&base_id.node);
                    self.hl_reference_type(&bname, &base_id.node, DocumentHighlightKind::Read);
                }
                // TypeMap: record type declaration
                if let Some(key) = decl_key {
                    self.type_map.insert(key, DeclType::Type(TypeDeclInfo {
                        base: t.base.as_ref().map(|id| self.node_text(&id.node)),
                    }));
                }
                let ann = extract_annotations(&self.rope, t.node.start_position().row);
                self.file_symbols.types.push(TypeSym {
                    name: name.clone(),
                    base: t.base.as_ref().map(|id| self.node_text(&id.node)),
                    decl_index,
                    doc_comment: ann.doc_comment,
                    ignore_tags: ann.ignore_tags,
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
                let native_decl_key = if let Some(ref name_id) = n.name {
                    Some(self.hl_declare_func(&name, &name_id.node))
                } else {
                    None
                };
                // hl: reference parameter types & declare param vars for TypeMap
                let mut param_pairs = Vec::new();
                for p in &n.params {
                    if let Some(ref tid) = p.type_id {
                        let tname = self.node_text(&tid.node);
                        self.hl_reference_type(&tname, &tid.node, DocumentHighlightKind::Read);
                    }
                    let pname = self.id_name(&p.name);
                    let ptype = p.type_id.as_ref().map(|id| self.node_text(&id.node)).unwrap_or_default();
                    param_pairs.push(ParamPair { name: pname, type_name: ptype });
                }
                // hl: reference return type
                if let Some(ref rt_id) = n.return_type {
                    let rt_name = self.node_text(&rt_id.node);
                    self.hl_reference_type(&rt_name, &rt_id.node, DocumentHighlightKind::Read);
                }
                // TypeMap: record native signature
                let return_type = n.return_type.as_ref().map(|id| self.node_text(&id.node));
                if let Some(key) = native_decl_key {
                    self.type_map.insert(key, DeclType::Func(FuncType {
                        params: param_pairs,
                        return_type: return_type.clone(),
                    }));
                }
                let ann = extract_annotations(&self.rope, n.node.start_position().row);
                self.file_symbols.natives.push(NativeSym {
                    name: name.clone(),
                    params: self.params_to_sym(&n.params),
                    return_type: return_type.clone(),
                    decl_index,
                    doc_comment: ann.doc_comment,
                    ignore_tags: ann.ignore_tags,
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
                let func_decl_key = if let Some(ref name_id) = f.name {
                    Some(self.hl_declare_func(&func_name, &name_id.node))
                } else {
                    None
                };
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
                let mut param_pairs = Vec::new();

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
                        let param_key = self.hl_declare_var(&pname, &name_id.node);
                        // TypeMap: record parameter
                        self.type_map.insert(param_key, DeclType::Var(VarType {
                            name: type_name.clone().unwrap_or_default(),
                            is_array: false,
                            is_constant: false,
                            is_comptime: false,
                        }));
                        param_pairs.push(ParamPair {
                            name: pname.clone(),
                            type_name: type_name.clone().unwrap_or_default(),
                        });
                        Self::scope_define(
                            &mut func_vars,
                            &self.node_text(&name_id.node),
                            name_id.node.start_byte(),
                            type_name.clone(),
                            false, false, true, true,
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

                // TypeMap: record function signature
                if let Some(key) = func_decl_key {
                    self.type_map.insert(key, DeclType::Func(FuncType {
                        params: param_pairs,
                        return_type: return_type.clone(),
                    }));
                }

                vars.push(func_vars);
                children.extend(self.visit_stmts(&f.body, vars));
                let func_vars = vars.pop().unwrap_or_default();

                let ann = extract_annotations(&self.rope, f.node.start_position().row);

                // Handle leak detection (respects //@ignore leak on the function)
                if !ann.ignore_tags.contains("leak") {
                    self.check_handle_leaks(&f.body, &func_vars, &f.node);
                }

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
                    doc_comment: ann.doc_comment,
                    ignore_tags: ann.ignore_tags,
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
                        let expr_type = if let Some(expr) = &d.value {
                            self.visit_expr(expr)
                        } else {
                            None
                        };
                        let var_name = self.id_name(&d.name);
                        let decl_index = self.next_decl_index();
                        let var_decl_key = if let Some(name_id) = &d.name {
                            // hl: declare global variable
                            let key = self.hl_declare_var(&var_name, &name_id.node);
                            Self::scope_define(
                                vars.last_mut().unwrap(),
                                &self.node_text(&name_id.node),
                                name_id.node.start_byte(),
                                type_name.clone(),
                                v.is_array, v.is_constant, d.value.is_some(), false,
                            );
                            Some(key)
                        } else {
                            None
                        };
                        // Type mismatch check: unknown → concrete type
                        if let (Some(tn), Some(et)) = (&type_name, &expr_type) {
                            self.check_type_mismatch(tn, Some(et.as_str()), &d.node);
                        }
                        let ann = extract_annotations(&self.rope, v.node.start_position().row);
                        self.file_symbols.globals.push(GlobalVarSym {
                            name: var_name.clone(),
                            type_name: type_name.clone(),
                            is_constant: v.is_constant,
                            is_array: v.is_array,
                            has_initializer: d.value.is_some(),
                            decl_index,
                            doc_comment: ann.doc_comment,
                            ignore_tags: ann.ignore_tags,
                        });
                        // type-tip: show type with const/comptime/array modifiers
                        if let Some(name_id) = &d.name {
                            if let Some(ref tn) = type_name {
                                let cv = d.value.as_ref().and_then(|e| self.eval_expr(e));
                                let is_comptime = v.is_constant && cv.is_some();
                                if is_comptime {
                                    if let Some(ref val) = cv {
                                        self.comptime_values.insert(var_name.clone(), val.clone());
                                    }
                                }
                                // TypeMap: record global variable type
                                if let Some(key) = var_decl_key {
                                    self.type_map.insert(key, DeclType::Var(VarType {
                                        name: tn.clone(),
                                        is_array: v.is_array,
                                        is_constant: v.is_constant,
                                        is_comptime,
                                    }));
                                }
                                let label = Self::build_type_label(
                                    tn, v.is_constant, is_comptime, v.is_array,
                                );
                                self.emit_type_hint(&name_id.node, &label, cv.as_ref());
                            }
                        }
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
                let expr_type = if let Some(expr) = &l.value {
                    self.visit_expr(expr)
                } else {
                    None
                };
                // Type mismatch check: unknown → concrete type
                if let Some(ref tid) = l.type_id {
                    let tn = self.node_text(&tid.node);
                    if let Some(ref et) = expr_type {
                        self.check_type_mismatch(&tn, Some(et.as_str()), &l.node);
                    }
                }
                if let (Some(scope), Some(name_id)) = (vars.last_mut(), &l.name) {
                    let lname = self.node_text(&name_id.node);
                    // hl: declare local variable
                    let local_key = self.hl_declare_var(&lname, &name_id.node);
                    // TypeMap: record local variable type
                    if let Some(ref tid) = l.type_id {
                        let tn = self.node_text(&tid.node);
                        self.type_map.insert(local_key, DeclType::Var(VarType {
                            name: tn.clone(),
                            is_array: l.is_array,
                            is_constant: false,
                            is_comptime: false,
                        }));
                        // type-tip: show local type + comptime value of initializer
                        let cv = l.value.as_ref().and_then(|e| self.eval_expr(e));
                        self.emit_type_hint(&name_id.node, &tn, cv.as_ref());
                    }
                    Self::scope_define(
                        scope,
                        &self.node_text(&name_id.node),
                        name_id.node.start_byte(),
                        l.type_id.as_ref().map(|id| self.node_text(&id.node)),
                        l.is_array, false, l.value.is_some(), false,
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
                // Inside a function body → treat as local variable declaration
                // (no `local` keyword required). In global scope → global variable.
                let in_function = self.current_callees.is_some();

                self.register_id(&v.type_id);
                let type_name = v.type_id.as_ref().map(|id| self.node_text(&id.node));
                // hl: reference the type
                if let Some(ref tid) = v.type_id {
                    let tname = self.node_text(&tid.node);
                    self.hl_reference_type(&tname, &tid.node, DocumentHighlightKind::Read);
                }
                for d in &v.decls {
                    self.register_id(&d.name);
                    let expr_type = if let Some(expr) = &d.value {
                        self.visit_expr(expr)
                    } else {
                        None
                    };
                    let var_name = self.id_name(&d.name);
                    let decl_index = self.next_decl_index();
                    let var_decl_key = if let Some(ref name_id) = d.name {
                        let vname = self.node_text(&name_id.node);
                        // hl: declare variable
                        let key = self.hl_declare_var(&vname, &name_id.node);
                        // If inside a function, register in the local scope vars map
                        if in_function {
                            if let Some(scope) = vars.last_mut() {
                                Self::scope_define(
                                    scope,
                                    &vname,
                                    name_id.node.start_byte(),
                                    type_name.clone(),
                                    v.is_array, v.is_constant, d.value.is_some(), false,
                                );
                            }
                        }
                        Some(key)
                    } else {
                        None
                    };
                    if !in_function {
                        // Export to file_symbols so the scope resolver makes
                        // this variable visible to importing files.
                        let ann = extract_annotations(&self.rope, v.node.start_position().row);
                        self.file_symbols.globals.push(GlobalVarSym {
                            name: var_name.clone(),
                            type_name: type_name.clone(),
                            is_constant: v.is_constant,
                            is_array: v.is_array,
                            has_initializer: d.value.is_some(),
                            decl_index,
                            doc_comment: ann.doc_comment,
                            ignore_tags: ann.ignore_tags,
                        });
                    }
                    // TypeMap + type-tip
                    if let Some(ref name_id) = d.name {
                        if let Some(ref tn) = type_name {
                            // Type mismatch check: unknown → concrete type
                            if let Some(ref et) = expr_type {
                                self.check_type_mismatch(tn, Some(et.as_str()), &d.node);
                            }
                            let cv = d.value.as_ref().and_then(|e| self.eval_expr(e));
                            let is_comptime = v.is_constant && cv.is_some();
                            if is_comptime {
                                let vname = self.node_text(&name_id.node);
                                if let Some(ref val) = cv {
                                    self.comptime_values.insert(vname, val.clone());
                                }
                            }
                            if let Some(key) = var_decl_key {
                                self.type_map.insert(key, DeclType::Var(VarType {
                                    name: tn.clone(),
                                    is_array: v.is_array,
                                    is_constant: v.is_constant,
                                    is_comptime,
                                }));
                            }
                            let label = Self::build_type_label(
                                tn, v.is_constant, is_comptime, v.is_array,
                            );
                            self.emit_type_hint(&name_id.node, &label, cv.as_ref());
                        }
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
                let value_type = if let Some(expr) = &s.value {
                    self.visit_expr(expr)
                } else {
                    None
                };
                if let Some(var_id) = &s.variable {
                    let name = self.node_text(&var_id.node);
                    // Type mismatch check: unknown → concrete type
                    if let Some(ref vt) = value_type {
                        if let Some(declared) = self.lookup_var_type(&name) {
                            self.check_type_mismatch(
                                &declared,
                                Some(vt.as_str()),
                                &s.node,
                            );
                        }
                    }
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
                for branch in &i.branches {
                    if let Some(cond) = &branch.condition {
                        self.visit_expr(cond);
                    }
                    let _body = self.visit_stmts(&branch.body, vars);
                }
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
            Statement::IgnoreDir(_) => unreachable!("handled above"),
            Statement::UjapiImport(_) => unreachable!("handled above"),
            Statement::Error(_) => None, // diagnostics already collected from ast.errors
        }
    }

    // ─── Expression type helpers ─────────────────────────────────────

    /// Check if a type name belongs to the handle family.
    ///
    /// Handle family = `handle` itself + any custom type that is not a
    /// built-in primitive (`integer`, `real`, `boolean`, `string`, `code`,
    /// `nothing`, `null`, `unknown`).
    fn is_handle_type(type_name: &str) -> bool {
        !matches!(
            type_name,
            "integer" | "real" | "boolean" | "string" | "code" | "nothing" | "null" | "unknown"
        )
    }

    /// Check if `expr_type` can be assigned to a variable of `declared_type`
    /// according to JASS type rules.
    ///
    /// Allowed implicit conversions:
    /// - same type → OK
    /// - `integer` → `real` (I2R)
    /// - `null` → any handle-based type, `string`, or `code`
    /// - any handle subtype → any other handle subtype (JASS allows implicit
    ///   handle casts)
    fn is_type_assignable(declared: &str, expr: &str) -> bool {
        // Same type is always OK.
        if declared == expr {
            return true;
        }

        // Unknown on either side → can't determine, assume OK.
        // The relevant "Undeclared" or operator diagnostics are emitted elsewhere.
        if declared == UNKNOWN_TYPE || expr == UNKNOWN_TYPE {
            return true;
        }

        // `nothing` is not assignable to/from anything.
        if expr == "nothing" || declared == "nothing" {
            return false;
        }

        // integer → real: OK (implicit I2R conversion).
        if expr == "integer" && declared == "real" {
            return true;
        }

        // null → string, code, or handle-based type: OK.
        if expr == "null" {
            return declared != "integer" && declared != "real" && declared != "boolean";
        }

        // Both handle-based → OK (JASS allows implicit handle casts).
        if Self::is_handle_type(declared) && Self::is_handle_type(expr) {
            return true;
        }

        // Everything else is a mismatch.
        false
    }

    /// Emit a diagnostic on the `=` operator when the inferred expression
    /// type is incompatible with the declared variable type.
    fn check_type_mismatch(
        &mut self,
        declared_type: &str,
        expr_type: Option<&str>,
        stmt_node: &Node,
    ) {
        if let Some(et) = expr_type {
            if !Self::is_type_assignable(declared_type, et) {
                let range = Self::find_equal_range(stmt_node, &self.rope)
                    .unwrap_or_else(|| stmt_node.to_range(&self.rope));
                self.diagnostics.push(Diagnostic {
                    range,
                    message: format!(
                        "Cannot assign type `{}` to `{}`",
                        et, declared_type,
                    ),
                    severity: Some(DiagnosticSeverity::Error),
                    ..Default::default()
                });
            }
        }
    }

    /// Look up the type of a variable by name via highlight scopes + type map,
    /// falling back to imported variable types.
    fn lookup_var_type(&self, name: &str) -> Option<String> {
        // Local scope lookup
        let decl_key = self
            .hl_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.vars.get(name).copied());
        if let Some(key) = decl_key {
            if let Some(DeclType::Var(vt)) = self.type_map.get(&key) {
                return Some(vt.name.clone());
            }
        }
        // Imported variable fallback
        self.imported_var_types
            .get(name)
            .and_then(|t| t.clone())
    }

    /// Look up the return type of a function by name via highlight scopes + type map,
    /// falling back to imported function return types.
    fn lookup_func_return_type(&self, name: &str) -> Option<String> {
        // Local scope lookup
        let decl_key = self
            .hl_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.funcs.get(name).copied());
        if let Some(key) = decl_key {
            if let Some(DeclType::Func(ft)) = self.type_map.get(&key) {
                return ft.return_type.clone();
            }
        }
        // Imported function fallback
        self.imported_func_returns
            .get(name)
            .and_then(|t| t.clone())
    }

    /// Find the operator token kind inside a binary expression CST node.
    fn binary_op_kind(node: &Node) -> Option<Kind> {
        let count = node.child_count();
        for i in 0..count {
            if let Some(child) = node.child(i as u32) {
                let k = Kind::try_from(child.grammar_id()).ok();
                match k {
                    Some(Kind::Plus) | Some(Kind::Minus) | Some(Kind::Star) | Some(Kind::Slash)
                    | Some(Kind::Lt) | Some(Kind::Gt) | Some(Kind::Le) | Some(Kind::Ge)
                    | Some(Kind::EqEq) | Some(Kind::Neq) | Some(Kind::And) | Some(Kind::Or) => {
                        return k;
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Find the operator **node** inside a binary expression CST node.
    fn binary_op_range(node: &Node, rope: &Rope) -> Option<(Kind, Range, String)> {
        let count = node.child_count();
        for i in 0..count {
            if let Some(child) = node.child(i as u32) {
                let k = Kind::try_from(child.grammar_id()).ok();
                match k {
                    Some(Kind::Plus) | Some(Kind::Minus) | Some(Kind::Star) | Some(Kind::Slash)
                    | Some(Kind::Lt) | Some(Kind::Gt) | Some(Kind::Le) | Some(Kind::Ge)
                    | Some(Kind::EqEq) | Some(Kind::Neq) | Some(Kind::And) | Some(Kind::Or) => {
                        let text_bytes = &rope.slice_to_cow(child.start_byte()..child.end_byte());
                        return Some((k.unwrap(), child.to_range(rope), text_bytes.to_string()));
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Find the `=` token range inside a CST node (declaration / set statement).
    fn find_equal_range(node: &Node, rope: &Rope) -> Option<Range> {
        let count = node.child_count();
        for i in 0..count {
            if let Some(child) = node.child(i as u32) {
                if Kind::try_from(child.grammar_id()).ok() == Some(Kind::Equal) {
                    return Some(child.to_range(rope));
                }
            }
        }
        None
    }

    /// Find the operator token kind for a unary expression CST node.
    fn unary_op_kind(node: &Node) -> Option<Kind> {
        node.child(0)
            .and_then(|c| Kind::try_from(c.grammar_id()).ok())
    }

    /// Infer the result type of a binary operation from operator and operand types.
    ///
    /// Returns `Some(UNKNOWN_TYPE)` when both operand types are known but
    /// the combination is invalid (e.g. `string * integer`, `boolean - boolean`),
    /// or when either operand is already `unknown`.
    fn infer_binary_type(
        op: Option<Kind>,
        left: Option<&str>,
        right: Option<&str>,
    ) -> Option<String> {
        let op = op?;
        let l = left?;
        let r = right?;

        // unknown propagates: any operation with unknown yields unknown.
        if l == UNKNOWN_TYPE || r == UNKNOWN_TYPE {
            return Some(UNKNOWN_TYPE.to_string());
        }

        let is_numeric = |t: &str| t == "integer" || t == "real";

        match op {
            // Comparison and logical operators always produce boolean.
            Kind::And | Kind::Or | Kind::Lt | Kind::Gt | Kind::Le | Kind::Ge
            | Kind::EqEq | Kind::Neq => Some("boolean".to_string()),

            Kind::Plus => {
                // string + anything ⇒ string (concatenation via I2S/R2S)
                if l == "string" || r == "string" {
                    Some("string".to_string())
                } else if is_numeric(l) && is_numeric(r) {
                    if l == "real" || r == "real" {
                        Some("real".to_string())
                    } else {
                        Some("integer".to_string())
                    }
                } else {
                    Some(UNKNOWN_TYPE.to_string())
                }
            }

            Kind::Minus | Kind::Star | Kind::Slash => {
                if is_numeric(l) && is_numeric(r) {
                    if l == "real" || r == "real" {
                        Some("real".to_string())
                    } else {
                        Some("integer".to_string())
                    }
                } else {
                    Some(UNKNOWN_TYPE.to_string())
                }
            }

            _ => None,
        }
    }

    // ─── Expression visitor ────────────────────────────────────────────

    /// Visit an expression, emitting type hints on leaf sub-expressions
    /// and returning the inferred type of the whole expression.
    fn visit_expr(&mut self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Id(id) => {
                self.id_roles.insert(id.node.start_byte(), id.role);
                let name = self.node_text(&id.node);
                self.hl_reference_var(&name, &id.node, DocumentHighlightKind::Read);

                // Infer type: built-in constants or variable lookup.
                // If the variable is not declared locally, return `unknown`
                // so the type propagates correctly through expressions.
                // A diagnostic will be emitted in Phase 2 if the name
                // is not resolved by imports either.
                let ty = match name.as_str() {
                    "true" | "false" => Some("boolean".to_string()),
                    "null" => Some("null".to_string()),
                    _ => self.lookup_var_type(&name)
                        .or_else(|| Some(UNKNOWN_TYPE.to_string())),
                };
                if let Some(ref t) = ty {
                    self.emit_type_hint(&id.node, t, None);
                }
                ty
            }
            Expr::Call(fc) => {
                self.register_id(&fc.name);
                let mut ret_type = None;
                if let Some(name_id) = &fc.name {
                    let fname = self.node_text(&name_id.node);
                    self.record_callee(&fname);
                    self.hl_reference_func(&fname, &name_id.node, DocumentHighlightKind::Read);
                    ret_type = self.lookup_func_return_type(&fname);
                }
                for arg in &fc.args {
                    self.visit_expr(arg);
                }
                if let Some(ref t) = ret_type {
                    self.emit_type_hint(&fc.node, t, None);
                }
                ret_type
            }
            Expr::FuncRef(id) => {
                self.id_roles.insert(id.node.start_byte(), id.role);
                let fname = self.node_text(&id.node);
                self.record_callee(&fname);
                self.hl_reference_func(&fname, &id.node, DocumentHighlightKind::Read);
                let ty = "code".to_string();
                self.emit_type_hint(&id.node, &ty, None);
                Some(ty)
            }
            Expr::Binary { node, left, right } => {
                let lt = self.visit_expr(left);
                let rt = self.visit_expr(right);
                let op = Self::binary_op_kind(node);
                let result = Self::infer_binary_type(op, lt.as_deref(), rt.as_deref());

                // Type error → diagnostic on the operator token
                if result.as_deref() == Some(UNKNOWN_TYPE) {
                    if let (Some(l), Some(r)) = (&lt, &rt) {
                        if let Some((_kind, op_range, op_text)) = Self::binary_op_range(node, &self.rope) {
                            self.diagnostics.push(Diagnostic {
                                range: op_range,
                                message: format!(
                                    "Operator `{}` cannot be applied to `{}` and `{}`",
                                    op_text, l, r
                                ),
                                severity: Some(DiagnosticSeverity::Error),
                                ..Default::default()
                            });
                        }
                    }
                }

                result
            }
            Expr::Unary { node, operand, .. } => {
                let ot = self.visit_expr(operand);
                let op = Self::unary_op_kind(node);
                let ot_type = ot.as_deref().map(|s| s.to_string());

                // unknown propagates through unary operations.
                let result = if ot.as_deref() == Some(UNKNOWN_TYPE) {
                    Some(UNKNOWN_TYPE.to_string())
                } else {
                    match (op, ot.as_deref()) {
                        (Some(Kind::Not), Some("boolean")) => Some("boolean".to_string()),
                        (Some(Kind::Not), Some(_)) => Some(UNKNOWN_TYPE.to_string()),
                        (Some(Kind::Minus), Some(t)) if t == "integer" || t == "real" => Some(t.to_string()),
                        (Some(Kind::Minus), Some(_)) => Some(UNKNOWN_TYPE.to_string()),
                        _ => ot,
                    }
                };

                // Type error → diagnostic on the operator token
                if result.as_deref() == Some(UNKNOWN_TYPE) {
                    if let Some(ref t) = ot_type {
                        if let Some(op_n) = node.child(0) {
                            let op_text = self.node_text(&op_n);
                            self.diagnostics.push(Diagnostic {
                                range: op_n.to_range(&self.rope),
                                message: format!(
                                    "Operator `{}` cannot be applied to `{}`",
                                    op_text, t
                                ),
                                severity: Some(DiagnosticSeverity::Error),
                                ..Default::default()
                            });
                        }
                    }
                }

                result
            }
            Expr::Parens { inner, .. } => {
                self.visit_expr(inner)
            }
            Expr::Index { array, index, .. } => {
                let arr_type = self.visit_expr(array);
                self.visit_expr(index);
                // Element type is the array variable's base type.
                arr_type
            }
            Expr::Literal(node) => {
                let kind = Kind::try_from(node.kind_id()).ok();
                let ty = match kind {
                    Some(Kind::Number) | Some(Kind::Rawcode) => Some("integer".to_string()),
                    Some(Kind::Float) => Some("real".to_string()),
                    Some(Kind::StringLiteral) => Some("string".to_string()),
                    _ => None,
                };
                if let Some(ref t) = ty {
                    self.emit_type_hint(node, t, None);
                }
                ty
            }
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
                            Kind::Id => {
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
                            | Kind::Constant | Kind::Array => TokenKind::Keyword,

                            Kind::And | Kind::Or | Kind::Not
                            | Kind::Equal | Kind::Comma
                            | Kind::LeftParen | Kind::RightParen
                            | Kind::LeftBracket | Kind::RightBracket
                            | Kind::Plus | Kind::Minus | Kind::Star | Kind::Slash
                            | Kind::PlusPlus | Kind::MinusMinus
                            | Kind::Lt | Kind::Gt | Kind::Le | Kind::Ge
                            | Kind::EqEq | Kind::Neq => TokenKind::Operator,

                            Kind::Number | Kind::Float | Kind::Rawcode => TokenKind::Number,
                            Kind::Comment => {
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
                                    // skip the default add_node below
                                    if cursor.goto_next_sibling() { continue; }
                                    while !cursor.goto_next_sibling() {
                                        if !cursor.goto_parent() { return; }
                                    }
                                    continue;
                                } else if trimmed.starts_with("//@ignore") {
                                    let prefix_len = "//@ignore".len();
                                    let ws_before = text.len() - trimmed.len();
                                    let abs_prefix = sb + ws_before;
                                    // Macro token for the "//@ignore" prefix (same as //set)
                                    self.semantic.add_range(abs_prefix, prefix_len, &self.rope, TokenKind::Macro, 0u32);
                                    // Each tag word as Property token (same as //set key)
                                    let after = &trimmed[prefix_len..];
                                    let mut byte_off = 0usize;
                                    for word in after.split_whitespace() {
                                        // find word start relative to `after`
                                        let wstart = after[byte_off..].find(word).unwrap() + byte_off;
                                        let abs_pos = abs_prefix + prefix_len + wstart;
                                        self.semantic.add_range(abs_pos, word.len(), &self.rope, TokenKind::Property, 0u32);
                                        byte_off = wstart + word.len();
                                    }
                                    if cursor.goto_next_sibling() { continue; }
                                    while !cursor.goto_next_sibling() {
                                        if !cursor.goto_parent() { return; }
                                    }
                                    continue;
                                }
                                TokenKind::Comment
                            }
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

    // ─── Handle leak detection ────────────────────────────────────────

    /// Check a function body for handle leaks.
    ///
    /// A **handle leak** occurs when a local variable of a handle type
    /// holds a non-null reference when the function exits.  JASS does not
    /// run destructors on local variables, so the reference count on the
    /// underlying handle is never decremented and the object lives forever.
    ///
    /// The analysis walks the function body and tracks a "nullified" set —
    /// variables known to be `null` at each point.  At every `return` and
    /// at the implicit return at `endfunction`, any handle local NOT in the
    /// set produces a warning.
    ///
    /// Control flow through `if` blocks is handled conservatively:
    /// a variable is considered nullified after an `if` only when it was
    /// nullified in **every** branch (if/elseif/else).  Without an `else`
    /// the `if` body's nullifications are not trusted.
    fn check_handle_leaks(
        &mut self,
        body: &[Statement],
        func_vars: &HashMap<String, VarInfo>,
        func_node: &Node,
    ) {
        // File-level `//ignore leak` suppresses all handle-leak diagnostics.
        if self.file_ignore_tags.contains("leak") {
            return;
        }

        // 1. Collect handle-type locals (not arrays — arrays don't leak).
        let mut handle_locals: Vec<HandleLocal> = Vec::new();
        for (name, info) in func_vars {
            if info.is_array || info.is_param {
                continue;
            }
            if let Some(ref tn) = info.type_name {
                if Self::is_handle_type(tn) {
                    // Per-variable `//@ignore leak` — check annotation above the local declaration.
                    let local_row = self.find_local_row(body, name);
                    if let Some(row) = local_row {
                        let ann = extract_annotations(&self.rope, row);
                        if ann.ignore_tags.contains("leak") {
                            continue;
                        }
                    }
                    let range = self.find_local_name_range(body, name)
                        .unwrap_or_else(|| func_node.to_range(&self.rope));
                    handle_locals.push(HandleLocal {
                        name: name.clone(),
                        type_name: tn.clone(),
                        range,
                        has_value: info.is_initialized,
                    });
                }
            }
        }

        if handle_locals.is_empty() {
            return;
        }

        // 2. Build initial null state map.
        //
        // ALL handle locals start as Null — even those with an initializer.
        // JASS hoists locals to the top of the function: at runtime,
        //   `local unit u = CreateUnit()`
        // becomes
        //   `local unit u`        (top, null)
        //   `set u = CreateUnit()` (at the original position)
        //
        // If there is a `return` before the local's source position, the
        // variable is still null there.  Starting with NonNull would be a
        // false negative — a missed leak.
        let mut null_map: NullMap = HashMap::new();
        for hl in &handle_locals {
            null_map.insert(hl.name.clone(), NullState::Null);
        }

        // 3. Walk the body tracking nullability at each exit point.
        let mut top_exits = Vec::new(); // exitwhen at top level is a syntax error
        let returned = self.walk_body_for_leaks(
            body,
            &mut null_map,
            &handle_locals,
            &mut top_exits,
        );

        // 4. If the function can fall through (no unconditional return),
        //    check for leaks at the implicit exit (endfunction).
        if !returned {
            let end_range = Self::endfunction_range(func_node, &self.rope);
            for hl in &handle_locals {
                let state = null_map.get(&hl.name).copied().unwrap_or(NullState::Null);
                if state != NullState::Null {
                    self.diagnostics.push(Diagnostic {
                        range: end_range.clone(),
                        message: format!(
                            "Handle leak: local `{}` (`{}`) is not set to `null` before function end",
                            hl.name, hl.type_name,
                        ),
                        severity: Some(DiagnosticSeverity::Error),
                        ..Default::default()
                    });
                }
            }
        }
    }

    /// Walk statements tracking nullability state of handle locals.
    /// Returns `true` if every code path ends with a `return`.
    ///
    /// `exit_collector` accumulates null-map snapshots at each `exitwhen`
    /// statement.  The caller (loop handler) uses them to compute the
    /// post-loop state — an `exitwhen` can leave the loop at a point
    /// where a variable is still non-null.
    ///
    /// ## Strict philosophy
    ///
    /// Any doubt is resolved toward "possibly non-null" → warning.
    /// Function calls cannot mutate locals (no pass-by-reference in JASS),
    /// so only explicit `set var = null` marks a variable as null.
    fn walk_body_for_leaks(
        &mut self,
        stmts: &[Statement],
        null_map: &mut NullMap,
        handle_locals: &[HandleLocal],
        exit_collector: &mut Vec<NullMap>,
    ) -> bool {
        for stmt in stmts {
            match stmt {
                Statement::Local(l) => {
                    if let Some(name_id) = &l.name {
                        let name = self.node_text(&name_id.node);
                        if handle_locals.iter().any(|hl| hl.name == name) {
                            if let Some(ref val) = l.value {
                                if Self::is_null_expr(val, &self.rope) {
                                    null_map.insert(name, NullState::Null);
                                } else {
                                    null_map.insert(name, NullState::NonNull);
                                }
                            } else {
                                // No initializer → local starts as null.
                                null_map.insert(name, NullState::Null);
                            }
                        }
                    }
                }
                Statement::Set(s) => {
                    if let Some(var_id) = &s.variable {
                        let name = self.node_text(&var_id.node);
                        if handle_locals.iter().any(|hl| hl.name == name) {
                            if let Some(ref val) = s.value {
                                if Self::is_null_expr(val, &self.rope) {
                                    null_map.insert(name, NullState::Null);
                                } else {
                                    null_map.insert(name, NullState::NonNull);
                                }
                            }
                        }
                    }
                }
                Statement::Exitwhen(_) => {
                    // Record the current state as a potential loop exit point.
                    // The enclosing loop handler will merge these with the
                    // pre-loop state.  We do NOT analyse the exitwhen condition
                    // (e.g. `exitwhen u == null`) — that would weaken the
                    // analysis.  Saving the full pre-exitwhen state is strict:
                    // any variable that is NonNull here will propagate to the
                    // post-loop state.
                    exit_collector.push(null_map.clone());
                }
                Statement::Return(r) => {
                    // Emit leak warnings for any handle local that isn't
                    // definitely null at this return statement.
                    let ret_range = Self::return_keyword_range(&r.node, &self.rope);
                    for hl in handle_locals {
                        let state = null_map.get(&hl.name).copied().unwrap_or(NullState::Null);
                        if state != NullState::Null {
                            self.diagnostics.push(Diagnostic {
                                range: ret_range.clone(),
                                message: format!(
                                    "Handle leak: local `{}` (`{}`) is not set to `null` before `return`",
                                    hl.name, hl.type_name,
                                ),
                                severity: Some(DiagnosticSeverity::Error),
                                ..Default::default()
                            });
                        }
                    }
                    return true;
                }
                Statement::If(i) => {
                    let has_else = i.branches.iter().any(|b| b.condition.is_none());

                    let mut all_return = true;
                    let mut merged: Option<NullMap> = None;
                    let mut returning_guards: Vec<NullGuard> = Vec::new();

                    // Helper: process one branch (condition + body).
                    let mut process_branch = |cond: Option<&Expr>,
                                               body: &[Statement],
                                               null_map: &NullMap,
                                               this: &mut Self|
                     -> (bool, NullMap, Option<NullGuard>) {
                        let mut branch_map = null_map.clone();
                        let guard = cond.and_then(|c| Self::extract_null_guard(c, &this.rope));
                        if let Some(ref g) = guard {
                            if handle_locals.iter().any(|hl| hl.name == g.var_name) {
                                let state = if g.is_neq { NullState::NonNull } else { NullState::Null };
                                branch_map.insert(g.var_name.clone(), state);
                            }
                        }
                        let returned = this.walk_body_for_leaks(
                            body, &mut branch_map, handle_locals, exit_collector,
                        );
                        (returned, branch_map, guard)
                    };

                    // First branch (if ... then ...).
                    {
                        let (returned, branch_map, guard) =
                            process_branch(i.condition.as_ref(), &i.body, null_map, self);
                        if returned {
                            if let Some(g) = guard { returning_guards.push(g); }
                        } else {
                            all_return = false;
                            merged = Some(branch_map);
                        }
                    }

                    // Subsequent branches (elseif / else).
                    for branch in &i.branches {
                        let (returned, branch_map, guard) =
                            process_branch(branch.condition.as_ref(), &branch.body, null_map, self);
                        if returned {
                            if let Some(g) = guard { returning_guards.push(g); }
                        } else {
                            all_return = false;
                            merged = Some(match merged {
                                Some(acc) => Self::join_null_maps(&acc, &branch_map),
                                None => branch_map,
                            });
                        }
                    }

                    if all_return && has_else {
                        return true;
                    }

                    // Without `else` the `if` could be skipped entirely, so
                    // the pre-if state is another "non-returning path".
                    if !has_else {
                        merged = Some(match merged {
                            Some(acc) => Self::join_null_maps(&acc, null_map),
                            None => null_map.clone(),
                        });
                    }

                    // Apply continuation state from merged branches.
                    if let Some(ref m) = merged {
                        for (name, state) in m {
                            null_map.insert(name.clone(), *state);
                        }
                    }

                    // Apply negation of guards from returning branches.
                    for guard in &returning_guards {
                        if handle_locals.iter().any(|hl| hl.name == guard.var_name) {
                            let negated = if guard.is_neq {
                                NullState::Null
                            } else {
                                NullState::NonNull
                            };
                            null_map.insert(guard.var_name.clone(), negated);
                        }
                    }
                }
                Statement::Loop(l) => {
                    // Fresh collector for this loop's exitwhen states.
                    let mut loop_exits: Vec<NullMap> = Vec::new();
                    let mut loop_map = null_map.clone();
                    let loop_returned = self.walk_body_for_leaks(
                        &l.body,
                        &mut loop_map,
                        handle_locals,
                        &mut loop_exits,
                    );

                    // After the loop, the state is the merge of ALL possible
                    // exit paths:
                    //
                    //  1. pre-loop — the loop might not execute at all
                    //     (e.g. `exitwhen true` as first statement)
                    //  2. each exitwhen snapshot — the loop exits mid-body,
                    //     variables may still be non-null
                    //  3. post-body fall-through (if the body doesn't always
                    //     return) — represents exitwhen at the TOP of the
                    //     next iteration
                    //
                    // This is strict: any variable that is NonNull at ANY
                    // potential exit point becomes at least MaybeNull.
                    let mut result = null_map.clone(); // (1) pre-loop
                    for exit_map in &loop_exits {      // (2) exitwhen snapshots
                        result = Self::join_null_maps(&result, exit_map);
                    }
                    if !loop_returned {                // (3) post-body
                        result = Self::join_null_maps(&result, &loop_map);
                    }
                    for (name, state) in result {
                        null_map.insert(name, state);
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Merge two null maps: for each variable, join their states.
    fn join_null_maps(a: &NullMap, b: &NullMap) -> NullMap {
        let mut result = a.clone();
        for (name, b_state) in b {
            let a_state = a.get(name).copied().unwrap_or(NullState::Null);
            result.insert(name.clone(), NullState::join(a_state, *b_state));
        }
        result
    }

    /// Extract the range of only the `return` keyword from a `return_statement` node.
    /// Falls back to the entire node range if the keyword child is not found.
    fn return_keyword_range(node: &Node, rope: &Rope) -> Range {
        let count = node.child_count();
        for i in 0..count {
            if let Some(child) = node.child(i as u32) {
                if Kind::try_from(child.grammar_id()) == Ok(Kind::Return) {
                    return child.to_range(rope);
                }
            }
        }
        node.to_range(rope)
    }

    /// Check if an expression is literally `null`.
    fn is_null_expr(expr: &Expr, rope: &Rope) -> bool {
        match expr {
            Expr::Id(id) => {
                let text = id.node.text(rope);
                text == "null"
            }
            _ => false,
        }
    }


    /// Try to extract a `var == null` or `var != null` pattern from an expression.
    fn extract_null_guard(expr: &Expr, rope: &Rope) -> Option<NullGuard> {
        match expr {
            Expr::Binary { node, left, right } => {
                let op = Self::binary_op_kind(node)?;
                let (var_name, is_neq) = match op {
                    Kind::Neq => {
                        // `var != null` or `null != var`
                        if Self::is_null_expr(right, rope) {
                            if let Expr::Id(id) = left.as_ref() {
                                (id.node.text(rope).to_string(), true)
                            } else {
                                return None;
                            }
                        } else if Self::is_null_expr(left, rope) {
                            if let Expr::Id(id) = right.as_ref() {
                                (id.node.text(rope).to_string(), true)
                            } else {
                                return None;
                            }
                        } else {
                            return None;
                        }
                    }
                    Kind::EqEq => {
                        // `var == null` or `null == var`
                        if Self::is_null_expr(right, rope) {
                            if let Expr::Id(id) = left.as_ref() {
                                (id.node.text(rope).to_string(), false)
                            } else {
                                return None;
                            }
                        } else if Self::is_null_expr(left, rope) {
                            if let Expr::Id(id) = right.as_ref() {
                                (id.node.text(rope).to_string(), false)
                            } else {
                                return None;
                            }
                        } else {
                            return None;
                        }
                    }
                    _ => return None,
                };
                Some(NullGuard { var_name, is_neq })
            }
            Expr::Parens { inner, .. } => Self::extract_null_guard(inner, rope),
            _ => None,
        }
    }

    /// Find the range of a local variable's name identifier in the function body.
    fn find_local_name_range(&self, body: &[Statement], name: &str) -> Option<Range> {
        for stmt in body {
            if let Statement::Local(l) = stmt {
                if let Some(name_id) = &l.name {
                    if self.node_text(&name_id.node) == name {
                        return Some(name_id.node.to_range(&self.rope));
                    }
                }
            }
        }
        None
    }

    /// Find the CST row of a local variable declaration in the function body.
    fn find_local_row(&self, body: &[Statement], name: &str) -> Option<usize> {
        for stmt in body {
            if let Statement::Local(l) = stmt {
                if let Some(name_id) = &l.name {
                    if self.node_text(&name_id.node) == name {
                        return Some(l.node.start_position().row);
                    }
                }
            }
        }
        None
    }

    /// Get the range of the `endfunction` keyword for a FunctionStatement CST node.
    fn endfunction_range(func_node: &Node, rope: &Rope) -> Range {
        let count = func_node.child_count();
        for i in (0..count).rev() {
            if let Some(child) = func_node.child(i as u32) {
                if Kind::try_from(child.grammar_id()).ok() == Some(Kind::Endfunction) {
                    return child.to_range(rope);
                }
            }
        }
        func_node.to_range(rope)
    }
}

