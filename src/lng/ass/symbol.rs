//! Owned (lifetime-free) symbol types for AngelScript files.
//!
//! Mirrors `lng::jass::symbol` but supports AS-specific constructs:
//! classes, interfaces, enums, mixins, typedefs, funcdefs, and namespaces.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use url::Url;

// ─── Parameter ──────────────────────────────────────────────────────────────

/// A function/method parameter: `type name`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSym {
    pub name: String,
    pub type_name: String,
}

// ─── Function / method ──────────────────────────────────────────────────────

/// A free function or method declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSym {
    pub name: String,
    pub params: Vec<ParamSym>,
    pub return_type: Option<String>,
    /// Enclosing namespace (empty string for top-level).
    pub namespace: String,
    pub doc_comment: Option<String>,
    /// `start_byte` of the declaring node — used as `decl_key`.
    pub decl_byte: usize,
}

// ─── Class member ───────────────────────────────────────────────────────────

/// A method inside a class / interface / mixin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodSym {
    pub name: String,
    pub params: Vec<ParamSym>,
    pub return_type: Option<String>,
    pub doc_comment: Option<String>,
    pub decl_byte: usize,
}

/// A property (field) inside a class / mixin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySym {
    pub name: String,
    pub type_name: Option<String>,
    pub doc_comment: Option<String>,
    pub decl_byte: usize,
}

// ─── Class ──────────────────────────────────────────────────────────────────

/// A `class` declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassSym {
    pub name: String,
    pub namespace: String,
    pub doc_comment: Option<String>,
    pub decl_byte: usize,
    #[serde(default)]
    pub methods: Vec<MethodSym>,
    #[serde(default)]
    pub properties: Vec<PropertySym>,
}

// ─── Interface ──────────────────────────────────────────────────────────────

/// An `interface` declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceSym {
    pub name: String,
    pub namespace: String,
    pub doc_comment: Option<String>,
    pub decl_byte: usize,
    #[serde(default)]
    pub methods: Vec<MethodSym>,
}

// ─── Enum ───────────────────────────────────────────────────────────────────

/// An `enum` declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumSym {
    pub name: String,
    pub namespace: String,
    pub doc_comment: Option<String>,
    pub decl_byte: usize,
    pub members: Vec<String>,
}

// ─── Mixin ──────────────────────────────────────────────────────────────────

/// A `mixin class` declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixinSym {
    pub name: String,
    pub namespace: String,
    pub doc_comment: Option<String>,
    pub decl_byte: usize,
    #[serde(default)]
    pub methods: Vec<MethodSym>,
    #[serde(default)]
    pub properties: Vec<PropertySym>,
}

// ─── Typedef ────────────────────────────────────────────────────────────────

/// A `typedef` declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedefSym {
    pub alias: String,
    pub original: String,
    pub namespace: String,
    pub doc_comment: Option<String>,
    pub decl_byte: usize,
}

// ─── Funcdef ────────────────────────────────────────────────────────────────

/// A `funcdef` declaration (delegate/callback signature).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuncdefSym {
    pub name: String,
    pub params: Vec<ParamSym>,
    pub return_type: Option<String>,
    pub namespace: String,
    pub doc_comment: Option<String>,
    pub decl_byte: usize,
}

// ─── Global variable ────────────────────────────────────────────────────────

/// A global variable (top-level or namespace-level).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalVarSym {
    pub name: String,
    pub type_name: Option<String>,
    pub namespace: String,
    pub doc_comment: Option<String>,
    pub decl_byte: usize,
}

// ─── Namespace ──────────────────────────────────────────────────────────────

/// A `namespace` declaration — name only (contents are flattened into the
/// per-symbol `namespace` field).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceSym {
    pub name: String,
    pub decl_byte: usize,
}

// ─── File-level symbol table ────────────────────────────────────────────────

/// All symbols declared in a single `.as` file.
///
/// Namespace-scoped symbols store the enclosing namespace name in their
/// `namespace` field.  Top-level symbols use `""`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AsFileSymbols {
    pub functions: Vec<FunctionSym>,
    pub classes: Vec<ClassSym>,
    pub interfaces: Vec<InterfaceSym>,
    pub enums: Vec<EnumSym>,
    pub mixins: Vec<MixinSym>,
    pub typedefs: Vec<TypedefSym>,
    pub funcdefs: Vec<FuncdefSym>,
    pub globals: Vec<GlobalVarSym>,
    pub namespaces: Vec<NamespaceSym>,

    /// URLs of files imported via `//import!` (frozen / read-only).
    pub frozen_imports: HashSet<Url>,
    /// Per-file settings parsed from `//set key value` directives.
    pub file_settings: HashMap<String, String>,
    /// File-level diagnostic suppression tags from `//ignore tag` directives.
    pub file_ignore_tags: HashSet<String>,
    /// `true` when the file contains a `//entry` directive —
    /// marks it as a build entry point for tree-shaking and import graph traversal.
    #[serde(default)]
    pub is_entry: bool,
}

#[allow(dead_code)]
impl AsFileSymbols {
    pub fn new() -> Self {
        Self::default()
    }

    /// Convert to the unified `FileSymbols` for storage in ParseSnapshot / file_cache.
    pub fn to_unified(&self) -> crate::lng::symbol::FileSymbols {
        use crate::lng::symbol as u;

        let conv_params = |params: &[ParamSym]| -> Vec<u::ParamSym> {
            params.iter().map(|p| u::ParamSym { name: p.name.clone(), type_name: p.type_name.clone() }).collect()
        };
        let conv_methods = |methods: &[MethodSym]| -> Vec<u::MethodSym> {
            methods.iter().map(|m| u::MethodSym {
                name: m.name.clone(),
                params: conv_params(&m.params),
                return_type: m.return_type.clone(),
                doc_comment: m.doc_comment.clone(),
                decl_byte: m.decl_byte,
            }).collect()
        };
        let conv_props = |props: &[PropertySym]| -> Vec<u::PropertySym> {
            props.iter().map(|p| u::PropertySym {
                name: p.name.clone(),
                type_name: p.type_name.clone(),
                doc_comment: p.doc_comment.clone(),
                decl_byte: p.decl_byte,
            }).collect()
        };

        u::FileSymbols {
            lang: u::Lang::As,
            functions: self.functions.iter().map(|f| u::FunctionSym {
                name: f.name.clone(),
                params: conv_params(&f.params),
                return_type: f.return_type.clone(),
                namespace: f.namespace.clone(),
                decl_byte: f.decl_byte,
                doc_comment: f.doc_comment.clone(),
                ..Default::default()
            }).collect(),
            classes: self.classes.iter().map(|c| u::ClassSym {
                name: c.name.clone(), namespace: c.namespace.clone(),
                doc_comment: c.doc_comment.clone(), decl_byte: c.decl_byte,
                methods: conv_methods(&c.methods),
                properties: conv_props(&c.properties),
            }).collect(),
            interfaces: self.interfaces.iter().map(|i| u::InterfaceSym {
                name: i.name.clone(), namespace: i.namespace.clone(),
                doc_comment: i.doc_comment.clone(), decl_byte: i.decl_byte,
                methods: conv_methods(&i.methods),
            }).collect(),
            enums: self.enums.iter().map(|e| u::EnumSym {
                name: e.name.clone(), namespace: e.namespace.clone(),
                doc_comment: e.doc_comment.clone(), decl_byte: e.decl_byte,
                members: e.members.clone(),
            }).collect(),
            mixins: self.mixins.iter().map(|m| u::MixinSym {
                name: m.name.clone(), namespace: m.namespace.clone(),
                doc_comment: m.doc_comment.clone(), decl_byte: m.decl_byte,
                methods: conv_methods(&m.methods),
                properties: conv_props(&m.properties),
            }).collect(),
            typedefs: self.typedefs.iter().map(|t| u::TypedefSym {
                alias: t.alias.clone(), original: t.original.clone(),
                namespace: t.namespace.clone(), doc_comment: t.doc_comment.clone(),
                decl_byte: t.decl_byte,
            }).collect(),
            funcdefs: self.funcdefs.iter().map(|f| u::FuncdefSym {
                name: f.name.clone(),
                params: conv_params(&f.params),
                return_type: f.return_type.clone(), namespace: f.namespace.clone(),
                doc_comment: f.doc_comment.clone(), decl_byte: f.decl_byte,
            }).collect(),
            globals: self.globals.iter().map(|g| u::GlobalVarSym {
                name: g.name.clone(), type_name: g.type_name.clone(),
                namespace: g.namespace.clone(), decl_byte: g.decl_byte,
                doc_comment: g.doc_comment.clone(),
                ..Default::default()
            }).collect(),
            namespaces: self.namespaces.iter().map(|n| u::NamespaceSym {
                name: n.name.clone(), decl_byte: n.decl_byte,
            }).collect(),
            frozen_imports: self.frozen_imports.clone(),
            file_settings: self.file_settings.clone(),
            file_ignore_tags: self.file_ignore_tags.clone(),
            is_entry: self.is_entry,
            ..Default::default()
        }
    }
}

