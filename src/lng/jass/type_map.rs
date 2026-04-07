//! Per-declaration type information — the foundation for type checking,
//! compile-time evaluation, implicit casts, and inlay hints.
//!
//! The [`TypeMap`] is built once per parse inside `Cursor::walk` and stored
//! in `ParseSnapshot`.  Every `DeclKey` that represents a typed entity
//! (variable, parameter, function, native, type alias) gets an entry.

use crate::http::ref_map::DeclKey;
use std::collections::HashMap;

// ─── Virtual / special type names ───────────────────────────────────────────

/// Name of the virtual type used when type inference fails
/// (e.g. `"hello" * 3`, `false - true`).
pub const UNKNOWN_TYPE: &str = "unknown";

// ─── Compile-time value ─────────────────────────────────────────────────────

/// A value fully evaluated at compile time.
///
/// Produced by `Cursor::eval_expr` for expressions built exclusively from
/// literals, other `comptime` globals, and pure operators.
#[derive(Debug, Clone, PartialEq)]
pub enum ComptimeValue {
    Integer(i64),
    Real(f64),
    Str(String),
    Bool(bool),
    Null,
}

impl std::fmt::Display for ComptimeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Integer(v) => write!(f, "{}", v),
            Self::Real(v) => {
                // Ensure at least one decimal place so it reads as a real.
                if v.fract() == 0.0 {
                    write!(f, "{:.1}", v)
                } else {
                    write!(f, "{}", v)
                }
            }
            Self::Str(v) => write!(f, "{}", v),
            Self::Bool(v) => write!(f, "{}", v),
            Self::Null => write!(f, "null"),
        }
    }
}

// ─── Atomic type descriptors ────────────────────────────────────────────────

/// Type of a variable, parameter, or global.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarType {
    /// Base type name (`"integer"`, `"real"`, `"handle"`, custom, …).
    pub name: String,
    /// Declared with `array`.
    pub is_array: bool,
    /// Declared with the JASS `constant` keyword.
    pub is_constant: bool,
    /// Value can be fully evaluated at compile time.
    ///
    /// True when the initialiser consists exclusively of literals, other
    /// `comptime` globals, and pure operators.  Only meaningful for
    /// `constant` globals — locals and parameters are never `comptime`.
    pub is_comptime: bool,
}

/// Signature of a `function` or `native`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncType {
    /// Parameter list in declaration order.
    pub params: Vec<ParamPair>,
    /// `None` ⇔ `returns nothing`.
    pub return_type: Option<String>,
}

/// One `(type, name)` pair in a parameter list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamPair {
    pub name: String,
    pub type_name: String,
}

/// A `type X extends Y` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDeclInfo {
    /// `None` when `extends` is missing or unresolved.
    pub base: Option<String>,
}

// ─── Unified enum ───────────────────────────────────────────────────────────

/// Resolved type information for any named entity in a JASS file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclType {
    /// Variable (global, local, or parameter).
    Var(VarType),
    /// Function or native.
    Func(FuncType),
    /// `type X extends Y`.
    Type(TypeDeclInfo),
}

// ─── TypeMap ────────────────────────────────────────────────────────────────

/// Per-file mapping from `DeclKey` → resolved type.
///
/// Built during `Cursor::walk`, stored in `ParseSnapshot`, consumed by
/// inlay hints, diagnostics, hover, build, and future type-checker passes.
#[derive(Debug, Clone, Default)]
pub struct TypeMap {
    pub entries: HashMap<DeclKey, DeclType>,
}

#[allow(dead_code)]
impl TypeMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: DeclKey, decl: DeclType) {
        self.entries.insert(key, decl);
    }

    pub fn get(&self, key: &DeclKey) -> Option<&DeclType> {
        self.entries.get(key)
    }
}

