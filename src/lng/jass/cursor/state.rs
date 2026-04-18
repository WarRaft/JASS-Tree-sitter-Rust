use std::collections::{HashMap, HashSet};

use crate::http::diagnostic::Diagnostic;
use crate::http::document_symbol::DocumentSymbol;
use crate::http::folding::FoldingRange;
use crate::http::inlay_hint::InlayHint;
use crate::http::ref_map::{DeclKey, ExternalDecl, RawOccurrence};
use crate::http::semantic::hub::Hub;
use crate::lng::jass::ast::IdRole;
use crate::lng::jass::type_map::{ComptimeValue, TypeMap};
use crate::lng::symbol::FileSymbols;
use lapce_xi_rope::Rope;

use super::{HlScope, UnresolvedRef};

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

    pub ref_groups: HashMap<DeclKey, Vec<RawOccurrence>>,
    pub ref_names: HashMap<DeclKey, String>,
    pub external_decls: HashMap<DeclKey, ExternalDecl>,

    pub func_decl_keys: HashSet<DeclKey>,
    pub var_decl_keys: HashSet<DeclKey>,
    pub arg_decl_keys: HashSet<DeclKey>,

    pub colors: Vec<crate::http::color::ColorInformation>,

    pub file_settings: HashMap<String, String>,
    pub file_ignore_tags: HashSet<String>,

    pub type_map: TypeMap,
    pub type_hints: Vec<InlayHint>,
    pub(super) comptime_values: HashMap<String, ComptimeValue>,
    pub(super) ast_comptime_values: HashMap<(usize, usize), ComptimeValue>,

    pub(super) rope: Rope,
    pub(super) id_roles: HashMap<usize, IdRole>,
    pub(super) directive_nodes: HashSet<usize>,
    pub(super) comment_start: Option<usize>,
    pub(super) comment_end: usize,
    pub(super) decl_counter: usize,
    pub(super) next_decl_key: DeclKey,
    pub(super) current_callees: Option<HashSet<String>>,
    pub bare_callees: HashSet<String>,
    pub(super) hl_scopes: Vec<HlScope>,
    pub(super) unresolved_refs: Vec<UnresolvedRef>,
    pub(super) imported_func_returns: HashMap<String, Option<String>>,
    pub(super) imported_var_types: HashMap<String, Option<String>>,
    pub(super) current_return_type: Option<String>,
}

