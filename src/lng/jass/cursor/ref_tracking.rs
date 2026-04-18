use std::collections::HashMap;
use crate::http::highlight::DocumentHighlightKind;
use crate::http::range::Range;
use crate::http::ref_map::{DeclKey, RawOccurrence};
use crate::util::roper::node::NodeExt;
use tree_sitter::Node;
use super::{Cursor, ImportedKind};

/// An unresolved reference collected during Phase 1 (local resolution).
/// Will be matched against imported symbols in Phase 2.
#[derive(Debug, Clone)]
pub struct UnresolvedRef {
    pub name: String,
    pub range: Range,
    pub kind: DocumentHighlightKind,
    /// Which namespace the reference lives in.
    pub namespace: ImportedKind,
    /// `true` when this reference comes from a **type** position.
    pub is_type_ref: bool,
}

/// Two-namespace scope: JASS separates variables and functions by name.
#[derive(Debug, Clone, Default)]
pub struct HlScope {
    pub vars: HashMap<String, DeclKey>,
    pub funcs: HashMap<String, DeclKey>,
}

impl Cursor {
    /// JASS built-in value literals that are not user-declared variables.
    pub(super) const BUILTIN_VALUES: &'static [&'static str] = &["true", "false", "null"];

    /// JASS built-in primitive types that have no user declaration.
    pub(super) const PRIMITIVE_TYPES: &'static [&'static str] = &[
        "integer", "real", "boolean", "string", "handle", "code", "nothing",
    ];

    /// Push a new highlight scope (e.g. entering a function body).
    pub(super) fn hl_push_scope(&mut self) {
        self.hl_scopes.push(HlScope::default());
    }

    /// Pop the innermost highlight scope (e.g. leaving a function body).
    pub(super) fn hl_pop_scope(&mut self) {
        self.hl_scopes.pop();
    }

    /// Declare a **variable** (global, local, param, constant).
    pub(super) fn hl_declare_var(&mut self, name: &str, node: &Node) -> DeclKey {
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
    pub(super) fn hl_declare_func(&mut self, name: &str, node: &Node) -> DeclKey {
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

    /// Declare a **type**. Types share the variable namespace for simplicity.
    pub(super) fn hl_declare_type(&mut self, name: &str, node: &Node) -> DeclKey {
        self.hl_declare_var(name, node)
    }

    /// Record a reference to a previously-declared **variable**.
    pub(super) fn hl_reference_var(&mut self, name: &str, node: &Node, kind: DocumentHighlightKind) {
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
                .push(RawOccurrence { range, kind, is_decl: false });
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

    /// Record a reference to a **type** name (shares the variable namespace).
    pub(super) fn hl_reference_type(&mut self, name: &str, node: &Node, kind: DocumentHighlightKind) {
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
                .push(RawOccurrence { range, kind, is_decl: false });
            self.ref_names.entry(key).or_insert_with(|| name.to_string());
        } else if !Self::PRIMITIVE_TYPES.contains(&name) {
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
    pub(super) fn hl_reference_func(&mut self, name: &str, node: &Node, kind: DocumentHighlightKind) {
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
                .push(RawOccurrence { range, kind, is_decl: false });
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
}

