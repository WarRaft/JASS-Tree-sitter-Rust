use std::collections::{HashMap, HashSet};

use crate::lng::ass::ast::*;
use crate::lng::ass::kind::Kind;
use crate::lng::ass::symbol::{
    AsFileSymbols, ClassSym, EnumSym, FuncdefSym, FunctionSym, GlobalVarSym,
    InterfaceSym, MethodSym, MixinSym, NamespaceSym, ParamSym, PropertySym, TypedefSym,
};
use crate::http::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::http::document_symbol::{DocumentSymbol, SymbolKind};
use crate::http::folding::{FoldingRange, FoldingRangeKind};
use crate::http::highlight::DocumentHighlightKind;
use crate::http::range::Range;
use crate::http::ref_map::{DeclKey, ExternalDecl, RawOccurrence, EXTERNAL_KEY_BASE};
use crate::http::semantic::hub::Hub;
use crate::http::semantic::token::Kind as TokenKind;
use crate::util::roper::node::NodeExt;
use lapce_xi_rope::Rope;
use tree_sitter::Node;
use url::Url;

// ─── Imported symbol descriptor ──────────────────────────────────────────────

/// Whether an imported symbol is a function or a variable/type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportedKind {
    Func,
    Var,
}

/// A symbol from an imported file visible in the current file.
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
    /// Return type for functions; `None` for variables.
    pub return_type: Option<String>,
    /// Type name for variables; `None` for functions.
    pub type_name: Option<String>,
    /// AS namespace (e.g. `"Jass"` for entities from `.j` files, `""` for top-level).
    pub namespace: String,
}

// ─── Scope types ─────────────────────────────────────────────────────────────

/// An unresolved reference collected during Phase 1 (local resolution).
#[derive(Debug, Clone)]
struct UnresolvedRef {
    name: String,
    range: Range,
    kind: DocumentHighlightKind,
    namespace: ImportedKind,
    is_type_ref: bool,
    /// Explicit namespace qualifier (e.g. `"Jass"` from `Jass::Foo`).
    /// `None` for unqualified references.
    qualifier: Option<String>,
    /// For `Something(args)` — byte offset of the callee identifier.
    /// When set, `link_imports` checks types first (constructor/cast)
    /// before falling back to function resolution. The resolved role is
    /// written into `id_roles`.
    call_site_byte: Option<usize>,
}

/// Three-namespace scope for AS: variables, functions, and types can
/// coexist with the same name.  Namespaces are tracked as type-level
/// declarations.
#[derive(Debug, Clone, Default)]
struct HlScope {
    vars: HashMap<String, DeclKey>,
    funcs: HashMap<String, DeclKey>,
    types: HashMap<String, DeclKey>,
    /// Known namespace names (from `namespace Foo { ... }` or `using namespace Foo`).
    namespaces: HashMap<String, DeclKey>,
}

// ─── Doc-comment extraction ──────────────────────────────────────────────────

/// Strip the `//*` prefix from a single comment line and return the doc text.
///
/// Rules:
/// - `//* foo` → `foo`   (strip `//*` + one trailing space)
/// - `//*foo`  → `foo`   (strip `//*` only)
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

/// Strip the `/** ... */` block-comment delimiters and return the doc body.
///
/// Handles both single-line (`/** text */`) and multi-line forms.
/// Leading `*` on continuation lines are stripped (Javadoc style).
fn strip_block_doc(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if !trimmed.starts_with("/**") {
        return None;
    }
    // Must end with `*/`
    if !trimmed.ends_with("*/") {
        return None;
    }
    // Exclude `/**/` — empty block doc
    let inner = &trimmed[3..trimmed.len() - 2];
    let mut lines = Vec::new();
    for line in inner.lines() {
        let stripped = line.trim();
        // Strip leading `*` (common Javadoc continuation)
        let stripped = if stripped.starts_with('*') {
            let after = &stripped[1..];
            if after.starts_with(' ') { &after[1..] } else { after }
        } else {
            stripped
        };
        lines.push(stripped.to_string());
    }
    // Trim leading/trailing blank lines
    while lines.first().map_or(false, |l| l.is_empty()) {
        lines.remove(0);
    }
    while lines.last().map_or(false, |l| l.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return None;
    }
    Some(lines.join("\n"))
}

/// Extract a doc comment from the comment block directly above a declaration
/// at the given `row`.
///
/// Supports two forms:
/// 1. `//*` single-line doc comments (consecutive lines joined).
/// 2. `/** ... */` block doc comments (Javadoc-style).
///
/// Walks upward from `row - 1`.  For `//*` lines, collects consecutive doc
/// lines.  For a block comment ending on `row - 1`, extracts the body.
fn extract_doc_comment(rope: &Rope, row: usize) -> Option<String> {
    if row == 0 {
        return None;
    }
    let line_count = rope.line_of_offset(rope.len()) + 1;

    // ── Phase 1: try consecutive `//*` lines walking upward ──────────
    let mut doc_lines = Vec::new();
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
        } else {
            break;
        }
    }
    if !doc_lines.is_empty() {
        doc_lines.reverse();
        return Some(doc_lines.join("\n"));
    }

    // ── Phase 2: try `/** ... */` block comment on the preceding line(s)
    // Look at the line just above the declaration and see if it (or the
    // block ending there) is a block-doc comment.
    let prev_row = row - 1;
    if prev_row >= line_count {
        return None;
    }
    let prev_line_start = rope.offset_of_line(prev_row);
    let prev_line_end = if prev_row + 1 < line_count {
        rope.offset_of_line(prev_row + 1)
    } else {
        rope.len()
    };
    let prev_line_text = rope.slice_to_cow(prev_line_start..prev_line_end);
    let prev_trimmed = prev_line_text.trim();
    // Single-line: `/** ... */` entirely on one line
    if prev_trimmed.starts_with("/**") && prev_trimmed.ends_with("*/") {
        return strip_block_doc(prev_trimmed);
    }
    // Multi-line: the line above ends with `*/` — scan backwards for `/**`
    if prev_trimmed.ends_with("*/") {
        let mut start_row = prev_row;
        while start_row > 0 {
            start_row -= 1;
            let ls = rope.offset_of_line(start_row);
            let le = if start_row + 1 < line_count {
                rope.offset_of_line(start_row + 1)
            } else {
                rope.len()
            };
            let lt = rope.slice_to_cow(ls..le);
            if lt.trim().starts_with("/**") {
                // Found the start — extract the whole block
                let block_end = prev_line_end.min(rope.len());
                let block_text = rope.slice_to_cow(ls..block_end);
                return strip_block_doc(&block_text);
            }
            // If we hit a line that isn't part of the block comment, stop
            if !lt.trim().starts_with('*') && !lt.trim().is_empty() {
                break;
            }
        }
    }
    None
}

// ─── Cursor ──────────────────────────────────────────────────────────────────

/// Two-phase AST visitor that collects all LSP data + scope info.
pub struct Cursor {
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: Vec<DocumentSymbol>,
    pub folding: Vec<FoldingRange>,
    pub semantic: Hub,
    pub file_symbols: AsFileSymbols,

    /// DeclKey → all raw occurrences (declaration + references).
    pub ref_groups: HashMap<DeclKey, Vec<RawOccurrence>>,
    /// DeclKey → symbol name.
    pub ref_names: HashMap<DeclKey, String>,
    /// Synthetic DeclKey → external declaration (for cross-file definition).
    pub external_decls: HashMap<DeclKey, ExternalDecl>,
    /// DeclKeys that belong to function declarations (not variables/types).
    pub func_decl_keys: HashSet<DeclKey>,
    /// DeclKeys that belong to variable declarations (globals + locals).
    pub var_decl_keys: HashSet<DeclKey>,
    /// DeclKeys that belong to function parameter declarations.
    pub arg_decl_keys: HashSet<DeclKey>,

    /// Color information for `|cAARRGGBB` in strings and `0xAARRGGBB` hex literals.
    pub colors: Vec<crate::http::color::ColorInformation>,

    /// Per-file settings parsed from `//set key value` directives.
    pub file_settings: HashMap<String, String>,
    /// File-level diagnostic suppression tags from `//ignore tag` directives.
    pub file_ignore_tags: HashSet<String>,

    // Working state
    rope: Rope,
    id_roles: HashMap<usize, IdRole>,
    directive_nodes: HashSet<usize>,
    comment_start: Option<usize>,
    comment_end: usize,
    next_decl_key: DeclKey,

    /// Name resolution stack. Last entry = innermost scope.
    hl_scopes: Vec<HlScope>,
    /// Unresolved references collected during Phase 1.
    unresolved_refs: Vec<UnresolvedRef>,
    /// Imported function return types: name → return_type.
    imported_func_returns: HashMap<String, Option<String>>,
    /// Imported variable types: name → type_name.
    imported_var_types: HashMap<String, Option<String>>,

    /// Namespace name stack for symbol collection.
    /// When visiting `namespace Foo { ... }`, `"Foo"` is pushed.
    namespace_stack: Vec<String>,
}

impl Cursor {
    /// Walk the AST in two phases, collecting everything.
    ///
    /// Phase 1: Walk AST → symbols, folding, id_roles, scopes, symbol collection.
    /// Phase 2: Link unresolved refs against imported symbols.
    pub fn walk(ast: &Ast, rope: &Rope, imported: &[ImportedSymbol]) -> Self {
        let mut c = Self {
            diagnostics: Vec::new(),
            symbols: Vec::new(),
            folding: Vec::new(),
            semantic: Hub::default(),
            file_symbols: AsFileSymbols::default(),
            ref_groups: HashMap::new(),
            ref_names: HashMap::new(),
            external_decls: HashMap::new(),
            func_decl_keys: HashSet::new(),
            var_decl_keys: HashSet::new(),
            arg_decl_keys: HashSet::new(),
            colors: Vec::new(),
            file_settings: HashMap::new(),
            file_ignore_tags: HashSet::new(),
            rope: rope.clone(),
            id_roles: HashMap::new(),
            directive_nodes: HashSet::new(),
            comment_start: None,
            comment_end: 0,
            next_decl_key: 0,
            hl_scopes: vec![HlScope::default()],
            unresolved_refs: Vec::new(),
            imported_func_returns: HashMap::new(),
            imported_var_types: HashMap::new(),
            namespace_stack: Vec::new(),
        };

        // Pre-populate imported type lookup maps.
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
                ..Diagnostic::new("as", "syntax")
            });
        }

        // Phase 1: Walk AST with local scopes
        c.symbols = c.visit_top_levels(&ast.items);

        // Phase 2: Link unresolved refs against imported symbols
        c.link_imports(imported);

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
            TopLevel::IgnoreDir(n) => n.node,
            TopLevel::UjapiDir(n) => n.node,
            TopLevel::EntryDir(n) => n.node,
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
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "<unnamed>".into())
    }

    fn id_sel_range(&self, id: &Option<Id>, fallback: &Node) -> Range {
        id.as_ref()
            .map(|id| id.node.to_range(&self.rope))
            .unwrap_or_else(|| fallback.to_range(&self.rope))
    }

    /// Current namespace string (dot-joined if nested), or `""` for top-level.
    fn current_namespace(&self) -> String {
        self.namespace_stack.join("::")
    }

    /// Check if an expression is the `this` or `super` keyword.
    fn expr_is_this(&self, expr: &Expr) -> bool {
        if let Expr::Id(id) = expr {
            let name = self.node_text(&id.node);
            return name == "this" || name == "super";
        }
        false
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

    // ─── DeclKey allocation ─────────────────────────────────────────────

    fn alloc_key(&mut self) -> DeclKey {
        let key = self.next_decl_key;
        self.next_decl_key += 1;
        key
    }

    // ─── Scope helpers ──────────────────────────────────────────────────

    fn hl_push_scope(&mut self) {
        self.hl_scopes.push(HlScope::default());
    }

    fn hl_pop_scope(&mut self) {
        self.hl_scopes.pop();
    }

    /// Declare a **variable** (global, local, param).
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

    /// Like [`hl_declare_var`] but reuses an existing key if the name was
    /// already pre-declared in the current scope (two-pass class resolution).
    fn hl_declare_var_or_reuse(&mut self, name: &str, node: &Node) -> DeclKey {
        if let Some(scope) = self.hl_scopes.last() {
            if let Some(&existing) = scope.vars.get(name) {
                return existing;
            }
        }
        self.hl_declare_var(name, node)
    }

    /// Declare a **function / method**.
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

    /// Like [`hl_declare_func`] but reuses an existing key if the name was
    /// already pre-declared in the current scope (two-pass class resolution).
    fn hl_declare_func_or_reuse(&mut self, name: &str, node: &Node) -> DeclKey {
        if let Some(scope) = self.hl_scopes.last() {
            if let Some(&existing) = scope.funcs.get(name) {
                return existing;
            }
        }
        self.hl_declare_func(name, node)
    }

    /// Declare a **type** (class, interface, enum, mixin, typedef, funcdef).
    fn hl_declare_type(&mut self, name: &str, node: &Node) -> DeclKey {
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
            scope.types.insert(name.to_string(), key);
        }
        key
    }

    /// Like [`hl_declare_type`] but reuses an existing key if the name was
    /// already pre-declared in the current scope (two-pass resolution).
    fn hl_declare_type_or_reuse(&mut self, name: &str, node: &Node) -> DeclKey {
        if let Some(scope) = self.hl_scopes.last() {
            if let Some(&existing) = scope.types.get(name) {
                return existing;
            }
        }
        self.hl_declare_type(name, node)
    }

    /// Declare a **namespace** name in the current scope.
    fn hl_declare_namespace(&mut self, name: &str, node: &Node) -> DeclKey {
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
            scope.namespaces.insert(name.to_string(), key);
        }
        key
    }

    /// Like [`hl_declare_namespace`] but reuses an existing key if the name
    /// was already pre-declared in the current scope (two-pass resolution).
    fn hl_declare_namespace_or_reuse(&mut self, name: &str, node: &Node) -> DeclKey {
        if let Some(scope) = self.hl_scopes.last() {
            if let Some(&existing) = scope.namespaces.get(name) {
                return existing;
            }
        }
        self.hl_declare_namespace(name, node)
    }

    /// AS built-in types that have no user declaration (primitives + template containers + funcdefs).
    const BUILTIN_TYPES: &'static [&'static str] = &[
        "void", "int", "int8", "int16", "int32", "int64",
        "uint", "uint8", "uint16", "uint32", "uint64",
        "float", "double", "bool", "string", "auto",
        "array", "dictionary", "table", "ref", "weakref", "const_weakref",
        // Built-in funcdef (delegate) types
        "CallbackFunc", "BoolexprFunc",
    ];

    /// Check if `name` is a known type — built-in, locally declared, or imported.
    ///
    /// Used to distinguish `TypeName(expr)` (type cast / constructor) from
    /// `funcName(expr)` (function call) in `Expr::Call`.
    fn is_known_type(&self, name: &str) -> bool {
        if Self::BUILTIN_TYPES.contains(&name) {
            return true;
        }
        // Check local scope type declarations (classes, interfaces, enums, mixins, typedefs)
        if self.hl_scopes.iter().rev().any(|scope| scope.types.contains_key(name)) {
            return true;
        }
        // Imported types arrive as ImportedKind::Var — check imported_var_types
        if self.imported_var_types.contains_key(name) {
            return true;
        }
        false
    }

    /// Recursively walk a `type` CST node (kind_id=193) and mark every
    /// inner identifier as `TypeRef`, calling `hl_reference_type` for each
    /// component name.  This correctly handles composite types such as
    /// `array<unit>`, `dictionary<string, int>`, `table<int, unit>`, etc.
    fn visit_type_node(&mut self, node: &Node) {
        let kind = Kind::try_from(node.kind_id()).ok();
        match kind {
            Some(Kind::Identifier) => {
                let name = self.node_text(node);
                self.hl_reference_type(&name, node, DocumentHighlightKind::Read);
                self.id_roles.insert(node.start_byte(), IdRole::TypeRef);
            }
            _ => {
                let count = node.child_count();
                for i in 0..count {
                    if let Some(child) = node.child(i as u32) {
                        self.visit_type_node(&child);
                    }
                }
            }
        }
    }

    /// Built-in value keywords that have no user declaration.
    /// These were previously hardcoded in the grammar but are now plain identifiers.
    const BUILTIN_VALUES: &'static [&'static str] = &[
        "null", "nil", "true", "false", "this", "super",
    ];

    /// Record a reference to a **variable**.
    fn hl_reference_var(&mut self, name: &str, node: &Node, hl_kind: DocumentHighlightKind) {
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
                    kind: hl_kind,
                    is_decl: false,
                });
            self.ref_names.entry(key).or_insert_with(|| name.to_string());
        } else if !Self::BUILTIN_VALUES.contains(&name) && !Self::BUILTIN_TYPES.contains(&name) {
            self.unresolved_refs.push(UnresolvedRef {
                name: name.to_string(),
                range: node.to_range(&self.rope),
                kind: hl_kind,
                namespace: ImportedKind::Var,
                is_type_ref: false,
                qualifier: None,
                call_site_byte: None,
            });
        }
    }

    /// Record a reference to a **type** name.
    fn hl_reference_type(&mut self, name: &str, node: &Node, hl_kind: DocumentHighlightKind) {
        // Check types first, then vars (types share the type namespace)
        let decl_key = self
            .hl_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.types.get(name).copied());

        if let Some(key) = decl_key {
            let range = node.to_range(&self.rope);
            self.ref_groups
                .entry(key)
                .or_default()
                .push(RawOccurrence {
                    range,
                    kind: hl_kind,
                    is_decl: false,
                });
            self.ref_names.entry(key).or_insert_with(|| name.to_string());
        } else if !Self::BUILTIN_TYPES.contains(&name) {
            self.unresolved_refs.push(UnresolvedRef {
                name: name.to_string(),
                range: node.to_range(&self.rope),
                kind: hl_kind,
                namespace: ImportedKind::Var,
                is_type_ref: true,
                qualifier: None,
                call_site_byte: None,
            });
        }
    }

    /// Record a reference to a **function**.
    fn hl_reference_func(&mut self, name: &str, node: &Node, hl_kind: DocumentHighlightKind) {
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
                    kind: hl_kind,
                    is_decl: false,
                });
            self.ref_names.entry(key).or_insert_with(|| name.to_string());
        } else {
            self.unresolved_refs.push(UnresolvedRef {
                name: name.to_string(),
                range: node.to_range(&self.rope),
                kind: hl_kind,
                namespace: ImportedKind::Func,
                is_type_ref: false,
                qualifier: None,
                call_site_byte: None,
            });
        }
    }

    /// Record a namespace-qualified reference: `ns::name`.
    fn hl_reference_ns_qualified(
        &mut self,
        ns_name: &str,
        ns_node: &Node,
        member_name: &str,
        member_node: &Node,
        is_func: bool,
    ) {
        // Reference the namespace name itself
        let ns_key = self
            .hl_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.namespaces.get(ns_name).copied());

        if let Some(key) = ns_key {
            let range = ns_node.to_range(&self.rope);
            self.ref_groups
                .entry(key)
                .or_default()
                .push(RawOccurrence {
                    range,
                    kind: DocumentHighlightKind::Read,
                    is_decl: false,
                });
            self.ref_names.entry(key).or_insert_with(|| ns_name.to_string());
        }

        // The member is always unresolved for now — will be matched in Phase 2
        // against imported symbols with the given namespace.
        self.unresolved_refs.push(UnresolvedRef {
            name: member_name.to_string(),
            range: member_node.to_range(&self.rope),
            kind: DocumentHighlightKind::Read,
            namespace: if is_func { ImportedKind::Func } else { ImportedKind::Var },
            is_type_ref: !is_func,
            qualifier: Some(ns_name.to_string()),
            call_site_byte: None,
        });
    }

    // ─── Phase 2: Import linking ────────────────────────────────────────

    /// Link unresolved references against local forward declarations,
    /// imported symbols, or standalone groups.
    fn link_imports(&mut self, imported: &[ImportedSymbol]) {
        use std::collections::HashMap as Map;
        use crate::http::ref_map::ExternalOrigin;

        // Build lookup: (name, kind, qualifier) → matching ImportedSymbols.
        // For qualifier=None, match entries with any namespace (unqualified access).
        // For qualifier=Some("Jass"), match entries where namespace == "Jass".
        let unresolved = std::mem::take(&mut self.unresolved_refs);
        let mut by_key: Map<(String, ImportedKind, Option<String>), Vec<UnresolvedRef>> = Map::new();
        for uref in unresolved {
            by_key
                .entry((uref.name.clone(), uref.namespace, uref.qualifier.clone()))
                .or_default()
                .push(uref);
        }

        let mut ext_counter: u32 = 0;

        for ((name, ns, qualifier), refs) in by_key {
            // Check if any ref in this group is a call-site (ambiguous
            // func-vs-constructor).  All refs in the same group share
            // (name, ns, qualifier) so the call-site flag propagates.
            let is_call_site = refs.iter().any(|r| r.call_site_byte.is_some());

            // 1. For unqualified refs, check local forward declarations.
            if qualifier.is_none() {
                let local_key = if let Some(scope) = self.hl_scopes.first() {
                    if is_call_site {
                        // Ambiguous call: check types first (constructor),
                        // then funcs.
                        scope.types.get(name.as_str()).copied()
                            .or_else(|| scope.funcs.get(name.as_str()).copied())
                    } else {
                        match ns {
                            ImportedKind::Func => scope.funcs.get(name.as_str()).copied(),
                            ImportedKind::Var => {
                                scope.types.get(name.as_str()).copied()
                                    .or_else(|| scope.vars.get(name.as_str()).copied())
                            }
                        }
                    }
                } else {
                    None
                };

                if let Some(key) = local_key {
                    // Determine whether the resolved key is a type
                    // (constructor) or a function, and fix up id_roles.
                    let resolved_as_type = self.hl_scopes.first()
                        .map(|scope| scope.types.get(name.as_str()).copied() == Some(key))
                        .unwrap_or(false);

                    for uref in &refs {
                        self.ref_groups
                            .entry(key)
                            .or_default()
                            .push(RawOccurrence {
                                range: uref.range.clone(),
                                kind: uref.kind,
                                is_decl: false,
                            });
                        if let Some(byte) = uref.call_site_byte {
                            self.id_roles.insert(
                                byte,
                                if resolved_as_type { IdRole::TypeRef } else { IdRole::FunctionCall },
                            );
                        }
                    }
                    continue;
                }
            }

            // 2. Match against imported symbols.
            let matching: Vec<&ImportedSymbol> = imported
                .iter()
                .filter(|sym| {
                    if sym.name != name { return false; }
                    let kind_matches = if is_call_site {
                        // Ambiguous call: accept both Func and Var (type)
                        // imports — we'll decide func-vs-constructor below.
                        true
                    } else {
                        match (ns, sym.kind) {
                            (ImportedKind::Func, ImportedKind::Func) => true,
                            (ImportedKind::Var, ImportedKind::Var) => true,
                            // funcdef is SymbolNS::Func but is also used as a type name,
                            // so type references (Var) must also match Func imports.
                            (ImportedKind::Var, ImportedKind::Func) => true,
                            _ => false,
                        }
                    };
                    if !kind_matches { return false; }

                    match &qualifier {
                        Some(q) => sym.namespace == *q,
                        None => true, // unqualified: match any namespace
                    }
                })
                .collect();

            if !matching.is_empty() {
                // For call-site refs, determine whether the import is a type
                // (constructor/cast) or a function.
                let resolved_as_type_import = is_call_site
                    && matching.iter().any(|sym| sym.kind == ImportedKind::Var);

                let key = EXTERNAL_KEY_BASE + ext_counter;
                ext_counter += 1;
                self.ref_names.insert(key, name.clone());

                let mut seen_uris = HashSet::new();
                let mut origins = Vec::new();
                for sym in &matching {
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
                for uref in &refs {
                    self.ref_groups
                        .entry(key)
                        .or_default()
                        .push(RawOccurrence {
                            range: uref.range.clone(),
                            kind: uref.kind,
                            is_decl: false,
                        });
                    if let Some(byte) = uref.call_site_byte {
                        self.id_roles.insert(
                            byte,
                            if resolved_as_type_import { IdRole::TypeRef } else { IdRole::FunctionCall },
                        );
                    }
                }
            } else {
                // 3. No match → standalone group + diagnostics.
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
                            is_decl: i == 0,
                        });
                    // For unresolved call-sites, default to FunctionCall.
                    if let Some(byte) = uref.call_site_byte {
                        self.id_roles.insert(byte, IdRole::FunctionCall);
                    }
                    self.diagnostics.push(Diagnostic {
                        range: uref.range.clone(),
                        message: crate::util::i18n::undeclared_symbol(
                            crate::util::i18n::undeclared_label(
                                uref.is_type_ref,
                                matches!(ns, ImportedKind::Func),
                            ),
                            &name,
                        ),
                        severity: Some(DiagnosticSeverity::Error),
                        ..Diagnostic::new("as", "undeclared")
                    });
                }
            }
        }
    }

    // ─── Top-level visitors ──────────────────────────────────────────────

    /// Recursively pre-declare all names at this scope level.
    /// Enters nested namespaces (push/pop child scopes) so their items
    /// are also pre-declared.  Does NOT resolve references or emit
    /// diagnostics — that happens in the full visit pass.
    fn predeclare_items(&mut self, items: &[TopLevel]) {
        for item in items {
            match item {
                TopLevel::Function(f) => {
                    if let Some(ref id) = f.name {
                        let name = self.node_text(&id.node);
                        self.hl_declare_func(&name, &id.node);
                    }
                }
                TopLevel::Class(cls) => {
                    if let Some(ref id) = cls.name {
                        let name = self.node_text(&id.node);
                        self.hl_declare_type(&name, &id.node);
                    }
                }
                TopLevel::Interface(iface) => {
                    if let Some(ref id) = iface.name {
                        let name = self.node_text(&id.node);
                        self.hl_declare_type(&name, &id.node);
                    }
                }
                TopLevel::Mixin(mx) => {
                    if let Some(ref id) = mx.name {
                        let name = self.node_text(&id.node);
                        self.hl_declare_type(&name, &id.node);
                    }
                }
                TopLevel::Enum(en) => {
                    if let Some(ref id) = en.name {
                        let name = self.node_text(&id.node);
                        self.hl_declare_type(&name, &id.node);
                    }
                }
                TopLevel::Typedef(td) => {
                    if let Some(ref id) = td.alias {
                        let name = self.node_text(&id.node);
                        self.hl_declare_type(&name, &id.node);
                    }
                }
                TopLevel::Funcdef(fd) => {
                    if let Some(ref id) = fd.name {
                        let name = self.node_text(&id.node);
                        self.hl_declare_type(&name, &id.node);
                    }
                }
                TopLevel::VarDecl(v) => {
                    for d in &v.decls {
                        if let Some(ref id) = d.name {
                            let dname = self.node_text(&id.node);
                            self.hl_declare_var(&dname, &id.node);
                        }
                    }
                }
                TopLevel::Namespace(ns) => {
                    // Pre-declare the namespace name in the parent scope.
                    // The namespace body items are pre-declared when
                    // visit_top_levels is called recursively for the body.
                    if let Some(ref id) = ns.name {
                        let ns_name = self.node_text(&id.node);
                        self.hl_declare_namespace(&ns_name, &id.node);
                    }
                }
                _ => {}
            }
        }
    }

    fn visit_top_levels(&mut self, items: &[TopLevel]) -> Vec<DocumentSymbol> {
        // ── Pass 0 (declaration collection): recursively pre-declare all
        // names at this scope level AND enter nested namespaces so that
        // their children are also pre-declared in child scopes.
        // This runs before any expression/reference resolution.
        self.predeclare_items(items);

        // ── Pass 1 (full visit): declarations reuse pre-registered keys.
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

        // SetDir directives
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

        // IgnoreDir directives
        if let TopLevel::IgnoreDir(ig) = item {
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

        // UjapiDir directives
        if let TopLevel::UjapiDir(ud) = item {
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

        // EntryDir directives
        if let TopLevel::EntryDir(ed) = item {
            self.flush_comment_run();
            self.directive_nodes.insert(ed.node.start_byte());
            crate::lng::directive::visit_entry_semantic(
                ed,
                &mut self.semantic,
                &self.rope,
            );
            self.file_symbols.is_entry = true;
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
                let ns_name = self.id_name(&ns.name);

                // Declare or reuse the namespace name (pre-declared in pass 0)
                if let Some(ref id) = ns.name {
                    self.hl_declare_namespace_or_reuse(&ns_name, &id.node);
                    self.id_roles.insert(id.node.start_byte(), IdRole::NamespaceDecl);
                }

                // Collect symbol
                self.file_symbols.namespaces.push(NamespaceSym {
                    name: ns_name.clone(),
                    decl_byte: ns.node.start_byte(),
                });

                self.push_fold_region(&ns.node);

                // Push namespace context for child symbols
                self.namespace_stack.push(ns_name.clone());
                self.hl_push_scope();
                let children = self.visit_top_levels(&ns.body);
                self.hl_pop_scope();
                self.namespace_stack.pop();

                Some(DocumentSymbol {
                    name: ns_name,
                    kind: SymbolKind::Namespace,
                    range: ns.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&ns.name, &ns.node),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }
            TopLevel::Typedef(td) => {
                let alias_name = self.id_name(&td.alias);
                let type_name = self.id_name(&td.type_id);

                if let Some(ref id) = td.alias {
                    self.hl_declare_type_or_reuse(&alias_name, &id.node);
                    self.id_roles.insert(id.node.start_byte(), IdRole::TypedefAlias);
                }
                if let Some(ref id) = td.type_id {
                    self.visit_type_node(&id.node);
                }

                self.file_symbols.typedefs.push(TypedefSym {
                    alias: alias_name.clone(),
                    original: type_name,
                    namespace: self.current_namespace(),
                    doc_comment: extract_doc_comment(&self.rope, td.node.start_position().row),
                    decl_byte: td.node.start_byte(),
                });

                Some(DocumentSymbol {
                    name: alias_name,
                    kind: SymbolKind::TypeParameter,
                    range: td.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&td.alias, &td.node),
                    ..Default::default()
                })
            }
            TopLevel::Funcdef(fd) => {
                let name = self.id_name(&fd.name);

                if let Some(ref id) = fd.name {
                    self.hl_declare_type_or_reuse(&name, &id.node);
                    self.id_roles.insert(id.node.start_byte(), IdRole::FuncdefName);
                }
                if let Some(ref id) = fd.return_type {
                    self.visit_type_node(&id.node);
                }
                for p in &fd.params {
                    if let Some(ref id) = p.type_id {
                        self.visit_type_node(&id.node);
                    }
                    self.register_id(&p.name);
                }

                self.file_symbols.funcdefs.push(FuncdefSym {
                    name: name.clone(),
                    params: self.params_to_sym(&fd.params),
                    return_type: fd.return_type.as_ref().map(|id| self.node_text(&id.node)),
                    namespace: self.current_namespace(),
                    doc_comment: extract_doc_comment(&self.rope, fd.node.start_position().row),
                    decl_byte: fd.node.start_byte(),
                });

                Some(DocumentSymbol {
                    name,
                    kind: SymbolKind::Function,
                    range: fd.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&fd.name, &fd.node),
                    ..Default::default()
                })
            }
            TopLevel::Enum(en) => {
                let name = self.id_name(&en.name);

                if let Some(ref id) = en.name {
                    self.hl_declare_type_or_reuse(&name, &id.node);
                    self.id_roles.insert(id.node.start_byte(), IdRole::EnumDecl);
                }

                self.push_fold_region(&en.node);
                let mut children = Vec::new();
                let mut member_names = Vec::new();
                for m in &en.members {
                    let mname = self.id_name(&m.name);
                    if let Some(ref id) = m.name {
                        self.hl_declare_var(&mname, &id.node);
                        self.id_roles.insert(id.node.start_byte(), IdRole::EnumMember);
                    }
                    if let Some(v) = &m.value {
                        self.visit_expr(v);
                    }
                    member_names.push(mname.clone());
                    children.push(DocumentSymbol {
                        name: mname,
                        kind: SymbolKind::EnumMember,
                        range: m.node.to_range(&self.rope),
                        selection_range: self.id_sel_range(&m.name, &m.node),
                        ..Default::default()
                    });
                }

                self.file_symbols.enums.push(EnumSym {
                    name: name.clone(),
                    namespace: self.current_namespace(),
                    doc_comment: extract_doc_comment(&self.rope, en.node.start_position().row),
                    decl_byte: en.node.start_byte(),
                    members: member_names,
                });

                Some(DocumentSymbol {
                    name,
                    kind: SymbolKind::Enum,
                    range: en.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&en.name, &en.node),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }
            TopLevel::Interface(iface) => {
                let name = self.id_name(&iface.name);

                if let Some(ref id) = iface.name {
                    self.hl_declare_type_or_reuse(&name, &id.node);
                    self.id_roles.insert(id.node.start_byte(), IdRole::InterfaceDecl);
                }

                self.push_fold_region(&iface.node);
                self.hl_push_scope();

                // Pass 1: pre-declare all interface methods
                for m in &iface.methods {
                    let method_name = self.id_name(&m.name);
                    if let Some(ref id) = m.name {
                        self.hl_declare_func(&method_name, &id.node);
                    }
                }

                // Pass 2: visit method bodies
                let mut children = Vec::new();
                let mut methods = Vec::new();
                for m in &iface.methods {
                    let method_name = self.id_name(&m.name);
                    methods.push(MethodSym {
                        name: method_name,
                        params: self.params_to_sym(&m.params),
                        return_type: m.return_type.as_ref().map(|id| self.node_text(&id.node)),
                        doc_comment: extract_doc_comment(&self.rope, m.node.start_position().row),
                        decl_byte: m.node.start_byte(),
                    });
                    if let Some(sym) = self.visit_function(m) {
                        children.push(sym);
                    }
                }
                self.hl_pop_scope();

                self.file_symbols.interfaces.push(InterfaceSym {
                    name: name.clone(),
                    namespace: self.current_namespace(),
                    doc_comment: extract_doc_comment(&self.rope, iface.node.start_position().row),
                    decl_byte: iface.node.start_byte(),
                    methods,
                });

                Some(DocumentSymbol {
                    name,
                    kind: SymbolKind::Interface,
                    range: iface.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&iface.name, &iface.node),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }
            TopLevel::Mixin(mx) => {
                let name = self.id_name(&mx.name);

                if let Some(ref id) = mx.name {
                    self.hl_declare_type_or_reuse(&name, &id.node);
                    self.id_roles.insert(id.node.start_byte(), IdRole::MixinDecl);
                }

                self.push_fold_region(&mx.node);
                self.hl_push_scope();
                let (children, methods, properties) = self.visit_class_members(&mx.members);
                self.hl_pop_scope();

                self.file_symbols.mixins.push(MixinSym {
                    name: name.clone(),
                    namespace: self.current_namespace(),
                    doc_comment: extract_doc_comment(&self.rope, mx.node.start_position().row),
                    decl_byte: mx.node.start_byte(),
                    methods,
                    properties,
                });

                Some(DocumentSymbol {
                    name,
                    kind: SymbolKind::Class,
                    range: mx.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&mx.name, &mx.node),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }
            TopLevel::Class(cls) => {
                let name = self.id_name(&cls.name);

                if let Some(ref id) = cls.name {
                    self.hl_declare_type_or_reuse(&name, &id.node);
                    self.id_roles.insert(id.node.start_byte(), IdRole::ClassDecl);
                }

                self.push_fold_region(&cls.node);
                self.hl_push_scope();
                let (children, methods, properties) = self.visit_class_members(&cls.members);
                self.hl_pop_scope();

                self.file_symbols.classes.push(ClassSym {
                    name: name.clone(),
                    namespace: self.current_namespace(),
                    doc_comment: extract_doc_comment(&self.rope, cls.node.start_position().row),
                    decl_byte: cls.node.start_byte(),
                    methods,
                    properties,
                });

                Some(DocumentSymbol {
                    name,
                    kind: SymbolKind::Class,
                    range: cls.node.to_range(&self.rope),
                    selection_range: self.id_sel_range(&cls.name, &cls.node),
                    children: if children.is_empty() { None } else { Some(children) },
                    ..Default::default()
                })
            }
            TopLevel::Function(f) => self.visit_function_decl(f, true),
            TopLevel::VarDecl(v) => self.visit_var_decl_top(v),
            TopLevel::Comment(_) => unreachable!("handled above"),
            TopLevel::ImportDir(_) => unreachable!("handled above"),
            TopLevel::SetDir(_) => unreachable!("handled above"),
            TopLevel::IgnoreDir(_) => unreachable!("handled above"),
            TopLevel::UjapiDir(_) => unreachable!("handled above"),
            TopLevel::EntryDir(_) => unreachable!("handled above"),
            TopLevel::Other(_) => None,
        }
    }

    // ─── Class member visitors ───────────────────────────────────────────

    fn visit_class_members(
        &mut self,
        members: &[ClassMember],
    ) -> (Vec<DocumentSymbol>, Vec<MethodSym>, Vec<PropertySym>) {
        let mut syms = Vec::new();
        let mut methods = Vec::new();
        let mut properties = Vec::new();

        // ── Pass 1: pre-declare ALL methods and properties into the class scope
        // so that every method body can see siblings declared below it.
        for m in members {
            match m {
                ClassMember::Function(f) => {
                    let method_name = self.id_name(&f.name);
                    if let Some(ref id) = f.name {
                        self.hl_declare_func(&method_name, &id.node);
                    }
                }
                ClassMember::Variable(v) => {
                    for d in &v.decls {
                        let dname = self.id_name(&d.name);
                        if let Some(ref id) = d.name {
                            self.hl_declare_var(&dname, &id.node);
                        }
                    }
                }
                ClassMember::Other(_) => {}
            }
        }

        // ── Pass 2: visit bodies — declarations reuse pre-registered keys.
        for m in members {
            match m {
                ClassMember::Function(f) => {
                    let method_name = self.id_name(&f.name);
                    methods.push(MethodSym {
                        name: method_name,
                        params: self.params_to_sym(&f.params),
                        return_type: f.return_type.as_ref().map(|id| self.node_text(&id.node)),
                        doc_comment: extract_doc_comment(&self.rope, f.node.start_position().row),
                        decl_byte: f.node.start_byte(),
                    });
                    if let Some(sym) = self.visit_function(f) {
                        syms.push(sym);
                    }
                }
                ClassMember::Variable(v) => {
                    let type_name = v.type_id.as_ref().map(|id| self.node_text(&id.node));
                    let var_doc = extract_doc_comment(&self.rope, v.node.start_position().row);
                    for d in &v.decls {
                        let dname = self.id_name(&d.name);
                        properties.push(PropertySym {
                            name: dname,
                            type_name: type_name.clone(),
                            doc_comment: var_doc.clone(),
                            decl_byte: d.node.start_byte(),
                        });
                    }
                    if let Some(sym) = self.visit_var_decl(v) {
                        syms.push(sym);
                    }
                }
                ClassMember::Other(_) => {}
            }
        }
        (syms, methods, properties)
    }

    /// Visit a function declaration inside a class/interface (no symbol export).
    fn visit_function(&mut self, f: &FunctionDecl) -> Option<DocumentSymbol> {
        self.visit_function_inner(f, false)
    }

    /// Visit a top-level function declaration (exported as symbol).
    fn visit_function_decl(&mut self, f: &FunctionDecl, export: bool) -> Option<DocumentSymbol> {
        self.visit_function_inner(f, export)
    }

    fn visit_function_inner(&mut self, f: &FunctionDecl, export: bool) -> Option<DocumentSymbol> {
        let name = self.id_name(&f.name);

        if let Some(ref id) = f.name {
            self.hl_declare_func_or_reuse(&name, &id.node);
            self.id_roles.insert(id.node.start_byte(), IdRole::FunctionDecl);
        }
        if let Some(ref id) = f.return_type {
            self.visit_type_node(&id.node);
        }
        self.push_fold_region(&f.node);

        // Parameters get their own scope
        self.hl_push_scope();
        let mut children = Vec::new();
        for p in &f.params {
            if let Some(ref id) = p.type_id {
                self.visit_type_node(&id.node);
            }
            if let Some(ref id) = p.name {
                let pname = self.node_text(&id.node);
                if pname.is_empty() { continue; }
                let pk = self.hl_declare_var(&pname, &id.node);
                self.arg_decl_keys.insert(pk);
                self.id_roles.insert(id.node.start_byte(), IdRole::Param);

                children.push(DocumentSymbol {
                    name: pname,
                    detail: p.type_id.as_ref().map(|id| self.node_text(&id.node)),
                    kind: SymbolKind::Variable,
                    range: p.node.to_range(&self.rope),
                    selection_range: id.node.to_range(&self.rope),
                    ..Default::default()
                });
            }
        }

        let body_syms = self.visit_stmts(&f.body);
        children.extend(body_syms);
        self.hl_pop_scope();

        // Collect symbol
        if export {
            self.file_symbols.functions.push(FunctionSym {
                name: name.clone(),
                params: self.params_to_sym(&f.params),
                return_type: f.return_type.as_ref().map(|id| self.node_text(&id.node)),
                namespace: self.current_namespace(),
                doc_comment: extract_doc_comment(&self.rope, f.node.start_position().row),
                decl_byte: f.node.start_byte(),
            });
        }

        Some(DocumentSymbol {
            name,
            kind: SymbolKind::Function,
            range: f.node.to_range(&self.rope),
            selection_range: self.id_sel_range(&f.name, &f.node),
            children: if children.is_empty() { None } else { Some(children) },
            ..Default::default()
        })
    }

    /// Visit a variable declaration inside a class (no symbol export).
    fn visit_var_decl(&mut self, v: &VarDeclStmt) -> Option<DocumentSymbol> {
        self.visit_var_decl_inner(v, false)
    }

    /// Visit a top-level variable declaration (exported as symbol).
    fn visit_var_decl_top(&mut self, v: &VarDeclStmt) -> Option<DocumentSymbol> {
        self.visit_var_decl_inner(v, true)
    }

    fn visit_var_decl_inner(&mut self, v: &VarDeclStmt, export: bool) -> Option<DocumentSymbol> {
        if let Some(ref id) = v.type_id {
            self.visit_type_node(&id.node);
        }

        let type_name = v.type_id.as_ref().map(|id| self.node_text(&id.node));

        for d in &v.decls {
            let dname = self.id_name(&d.name);
            if let Some(ref id) = d.name {
                let vk = self.hl_declare_var_or_reuse(&dname, &id.node);
                self.var_decl_keys.insert(vk);
                self.id_roles.insert(id.node.start_byte(), IdRole::Variable);
            }
            if let Some(val) = &d.value {
                self.visit_expr(val);
            }
            for arg in &d.args {
                self.visit_expr(arg);
            }

            // Collect symbol
            if export {
                self.file_symbols.globals.push(GlobalVarSym {
                    name: dname,
                    type_name: type_name.clone(),
                    namespace: self.current_namespace(),
                    doc_comment: extract_doc_comment(&self.rope, v.node.start_position().row),
                    decl_byte: d.node.start_byte(),
                });
            }
        }

        v.decls.first().map(|d| DocumentSymbol {
            name: self.id_name(&d.name),
            detail: type_name,
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
                self.hl_push_scope();
                self.visit_stmts(&i.body);
                self.hl_pop_scope();
                None
            }
            Stmt::While(w) => {
                self.push_fold_region(&w.node);
                if let Some(c) = &w.condition { self.visit_expr(c); }
                self.hl_push_scope();
                self.visit_stmts(&w.body);
                self.hl_pop_scope();
                None
            }
            Stmt::DoWhile(d) => {
                self.push_fold_region(&d.node);
                if let Some(c) = &d.condition { self.visit_expr(c); }
                self.hl_push_scope();
                self.visit_stmts(&d.body);
                self.hl_pop_scope();
                None
            }
            Stmt::For(f) => {
                self.push_fold_region(&f.node);
                self.hl_push_scope();
                // Visit init: declare variables or visit expression
                match &f.init {
                    Some(ForInit::VarDecl(v)) => { self.visit_var_decl(v); }
                    Some(ForInit::Expr(e)) => { self.visit_expr(e); }
                    None => {}
                }
                // Visit condition and update expressions
                if let Some(c) = &f.condition { self.visit_expr(c); }
                for u in &f.update { self.visit_expr(u); }
                self.visit_stmts(&f.body);
                self.hl_pop_scope();
                None
            }
            Stmt::Foreach(f) => {
                self.push_fold_region(&f.node);
                self.hl_push_scope();
                self.visit_stmts(&f.body);
                self.hl_pop_scope();
                None
            }
            Stmt::Switch(s) => {
                self.push_fold_region(&s.node);
                self.hl_push_scope();
                self.visit_stmts(&s.body);
                self.hl_pop_scope();
                None
            }
            Stmt::Try(t) => {
                self.push_fold_region(&t.node);
                self.hl_push_scope();
                self.visit_stmts(&t.body);
                self.hl_pop_scope();
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
                self.hl_push_scope();
                self.visit_stmts(stmts);
                self.hl_pop_scope();
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
                let name = self.node_text(&id.node);
                match id.role {
                    IdRole::Variable => {
                        self.hl_reference_var(&name, &id.node, DocumentHighlightKind::Read);
                    }
                    IdRole::TypeRef => {
                        self.hl_reference_type(&name, &id.node, DocumentHighlightKind::Read);
                    }
                    _ => {}
                }
                self.id_roles.insert(id.node.start_byte(), id.role);
            }
            Expr::Call { callee, callee_expr, args, .. } => {
                if let Some(id) = callee {
                    let name = self.node_text(&id.node);
                    // Check if this is a type constructor/cast: `TypeName(expr)`
                    if self.is_known_type(&name) {
                        self.hl_reference_type(&name, &id.node, DocumentHighlightKind::Read);
                        self.id_roles.insert(id.node.start_byte(), IdRole::TypeRef);
                    } else {
                        // Try local func scope first
                        let local_func = self
                            .hl_scopes
                            .iter()
                            .rev()
                            .find_map(|scope| scope.funcs.get(name.as_str()).copied());

                        if let Some(key) = local_func {
                            // Resolved to a locally-declared function.
                            let range = id.node.to_range(&self.rope);
                            self.ref_groups
                                .entry(key)
                                .or_default()
                                .push(RawOccurrence {
                                    range,
                                    kind: DocumentHighlightKind::Read,
                                    is_decl: false,
                                });
                            self.ref_names.entry(key).or_insert_with(|| name.to_string());
                            self.id_roles.insert(id.node.start_byte(), IdRole::FunctionCall);
                        } else {
                            // Check if it's a local variable (funcdef-typed variable
                            // being called, e.g. `DamageCallbackFn@ cb; cb(...);`).
                            let local_var = self
                                .hl_scopes
                                .iter()
                                .rev()
                                .find_map(|scope| scope.vars.get(name.as_str()).copied());

                            if let Some(key) = local_var {
                                let range = id.node.to_range(&self.rope);
                                self.ref_groups
                                    .entry(key)
                                    .or_default()
                                    .push(RawOccurrence {
                                        range,
                                        kind: DocumentHighlightKind::Read,
                                        is_decl: false,
                                    });
                                self.ref_names.entry(key).or_insert_with(|| name.to_string());
                                self.id_roles.insert(id.node.start_byte(), IdRole::Variable);
                            } else {
                                // Ambiguous: could be a forward-declared type
                                // constructor or an imported function.  Defer to
                                // link_imports (Phase 2) which checks types first.
                                self.unresolved_refs.push(UnresolvedRef {
                                    name: name.to_string(),
                                    range: id.node.to_range(&self.rope),
                                    kind: DocumentHighlightKind::Read,
                                    namespace: ImportedKind::Func,
                                    is_type_ref: false,
                                    qualifier: None,
                                    call_site_byte: Some(id.node.start_byte()),
                                });
                            }
                        }
                    }
                } else if let Some(expr) = callee_expr {
                    match expr.as_ref() {
                        // Namespace-qualified function call: Jass::Func(...)
                        Expr::NamespaceAccess { namespace, name, .. } => {
                            if let (Some(ns_id), Some(name_id)) = (namespace, name) {
                                let ns_name = self.node_text(&ns_id.node);
                                let member_name = self.node_text(&name_id.node);
                                self.hl_reference_ns_qualified(
                                    &ns_name, &ns_id.node,
                                    &member_name, &name_id.node,
                                    true,
                                );
                                self.id_roles.insert(ns_id.node.start_byte(), IdRole::NamespaceRef);
                                self.id_roles.insert(name_id.node.start_byte(), IdRole::FunctionCall);
                            }
                        }
                        // Member method call: obj.method(...) / this.method(...)
                        Expr::MemberAccess { object, member, .. } => {
                            let is_this = self.expr_is_this(object);
                            self.visit_expr(object);
                            if let Some(id) = member {
                                if is_this {
                                    let name = self.node_text(&id.node);
                                    self.hl_reference_func(&name, &id.node, DocumentHighlightKind::Read);
                                }
                                self.id_roles.insert(id.node.start_byte(), IdRole::FunctionCall);
                            }
                        }
                        _ => {}
                    }
                }
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            Expr::MemberAccess { object, member, .. } => {
                let is_this = self.expr_is_this(object);
                self.visit_expr(object);
                if let Some(id) = member {
                    if is_this {
                        let name = self.node_text(&id.node);
                        self.hl_reference_var(&name, &id.node, DocumentHighlightKind::Read);
                    }
                    self.id_roles.insert(id.node.start_byte(), IdRole::Property);
                }
            }
            Expr::NamespaceAccess { namespace, name, .. } => {
                if let (Some(ns_id), Some(name_id)) = (namespace, name) {
                    let ns_name = self.node_text(&ns_id.node);
                    let member_name = self.node_text(&name_id.node);
                    self.hl_reference_ns_qualified(
                        &ns_name, &ns_id.node,
                        &member_name, &name_id.node,
                        false, // default to var/type; could be refined
                    );
                    self.id_roles.insert(ns_id.node.start_byte(), IdRole::NamespaceRef);
                    self.id_roles.insert(name_id.node.start_byte(), IdRole::Variable);
                } else {
                    if let Some(id) = namespace {
                        self.id_roles.insert(id.node.start_byte(), IdRole::NamespaceRef);
                    }
                    if let Some(id) = name {
                        self.id_roles.insert(id.node.start_byte(), IdRole::Variable);
                    }
                }
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
            | Expr::Cast { .. } | Expr::New { .. }
            | Expr::Lambda { .. } | Expr::Other(_) => {}
            Expr::HandleOf { operand, .. } => {
                // `@FuncName` — function reference (compatible with JASS `code` type)
                // `@var = expr` — handle assignment (assign handle to a variable)
                // `@this` / `@super` — handle-to-self reference
                match operand.as_ref() {
                    Expr::Id(id) => {
                        let name = self.node_text(&id.node);
                        // Built-in keywords (`this`, `super`, etc.) — not a real reference.
                        if Self::BUILTIN_VALUES.contains(&name.as_str()) {
                            self.id_roles.insert(id.node.start_byte(), IdRole::Variable);
                        } else {
                            // Check if the name is a known function.
                            let is_func = self
                                .hl_scopes
                                .iter()
                                .rev()
                                .any(|scope| scope.funcs.contains_key(&name));
                            // Check if the name is a known variable.
                            let is_var = !is_func && self
                                .hl_scopes
                                .iter()
                                .rev()
                                .any(|scope| scope.vars.contains_key(&name));
                            if is_var {
                                // Handle assignment: @var = expr
                                self.hl_reference_var(&name, &id.node, DocumentHighlightKind::Write);
                                self.id_roles.insert(id.node.start_byte(), IdRole::Variable);
                            } else {
                                // Function reference (@FuncName) or unresolved forward reference
                                self.hl_reference_func(&name, &id.node, DocumentHighlightKind::Read);
                                self.id_roles.insert(id.node.start_byte(), IdRole::FunctionCall);
                            }
                        }
                    }
                    other => self.visit_expr(other),
                }
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

                // Char literal ('A'): integer value — mark as Number
                if kind == Some(Kind::CharLiteral) {
                    self.semantic.add_node(&node, &self.rope, TokenKind::Number, 0u32);
                    if cursor.goto_next_sibling() { continue; }
                    while !cursor.goto_next_sibling() {
                        if !cursor.goto_parent() { return; }
                    }
                    continue;
                }

                // String literal: tokenize with escape/color-code awareness
                if kind == Some(Kind::StringLiteral) {
                    crate::lng::string_colors::tokenize_string_literal(&node, &self.rope, &mut self.semantic);
                    self.colors.extend(crate::lng::string_colors::collect_string_colors(&node, &self.rope));
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
                                let text = self.rope.slice_to_cow(node.start_byte()..node.end_byte());
                                match text.as_ref() {
                                    // value literals
                                    "null" | "nil" | "true" | "false" => TokenKind::Number,
                                    // keyword-like identifiers
                                    "this" | "super" => TokenKind::Keyword,
                                    _ => if let Some(&role) = self.id_roles.get(&node.start_byte()) {
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
                            }

                            // keywords
                            Kind::HashInclude | Kind::Import | Kind::From | Kind::Using
                            | Kind::Namespace
                            | Kind::Typedef | Kind::Shared | Kind::Funcdef | Kind::External
                            | Kind::Enum | Kind::Interface | Kind::Mixin | Kind::Abstract
                            | Kind::Final | Kind::Class | Kind::Private | Kind::Protected
                            | Kind::Public | Kind::Override | Kind::Explicit | Kind::Const
                            | Kind::Delete | Kind::If | Kind::Else | Kind::While | Kind::Do
                            | Kind::For | Kind::In | Kind::Switch | Kind::Case | Kind::Default
                            | Kind::Return | Kind::Break | Kind::Continue | Kind::Try
                            | Kind::Catch | Kind::Throw | Kind::Cast | Kind::OpImplCast
                            | Kind::Function | Kind::New | Kind::Is | Kind::Not | Kind::And
                            | Kind::Or | Kind::Xor | Kind::Out | Kind::Inout => TokenKind::Keyword,

                            // literals
                            Kind::IntegerLiteral | Kind::HexLiteral | Kind::BitsLiteral
                            | Kind::FloatLiteral => {
                                // Collect color from hex literals like 0xAARRGGBB
                                if kind == Kind::HexLiteral {
                                    if let Some(ci) = crate::lng::string_colors::collect_hex_literal_color(&node, &self.rope) {
                                        self.colors.push(ci);
                                    }
                                }
                                TokenKind::Number
                            }

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
                                } else if trimmed.starts_with("/**") {
                                    // /** ... */ block doc comment: delimiters as Comment, body as String
                                    let ws_before = text.len() - trimmed.len();
                                    let abs_start = sb + ws_before;
                                    // Opening `/**`
                                    self.semantic.add_range(abs_start, 3, &self.rope, TokenKind::Comment, 0u32);
                                    // Closing `*/`
                                    let close_start = eb - 2;
                                    if close_start > abs_start + 3 {
                                        // Body between `/**` and `*/`
                                        self.semantic.add_range(abs_start + 3, close_start - (abs_start + 3), &self.rope, TokenKind::String, 0u32);
                                    }
                                    self.semantic.add_range(close_start, 2, &self.rope, TokenKind::Comment, 0u32);
                                    if cursor.goto_next_sibling() { continue; }
                                    while !cursor.goto_next_sibling() {
                                        if !cursor.goto_parent() { return; }
                                    }
                                    continue;
                                } else if trimmed.starts_with("//@ignore") {
                                    let prefix_len = "//@ignore".len();
                                    let ws_before = text.len() - trimmed.len();
                                    let abs_prefix = sb + ws_before;
                                    self.semantic.add_range(abs_prefix, prefix_len, &self.rope, TokenKind::Macro, 0u32);
                                    let after = &trimmed[prefix_len..];
                                    let mut byte_off = 0usize;
                                    for word in after.split_whitespace() {
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

