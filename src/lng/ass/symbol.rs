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

// ─── Class ──────────────────────────────────────────────────────────────────

/// A `class` declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassSym {
    pub name: String,
    pub namespace: String,
    pub doc_comment: Option<String>,
    pub decl_byte: usize,
}

// ─── Interface ──────────────────────────────────────────────────────────────

/// An `interface` declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceSym {
    pub name: String,
    pub namespace: String,
    pub doc_comment: Option<String>,
    pub decl_byte: usize,
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
}

#[allow(dead_code)]
impl AsFileSymbols {
    pub fn new() -> Self {
        Self::default()
    }
}

