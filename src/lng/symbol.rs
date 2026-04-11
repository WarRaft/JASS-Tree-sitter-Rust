//! Unified symbol types shared by JASS and AngelScript.
//!
//! JASS is treated as a restricted dialect of AS — the unified `FileSymbols`
//! struct is AS-centric (superset) and carries a `Lang` marker.
//!
//! JASS-only constructs (`NativeSym`, `TypeSym`) and AS-only constructs
//! (`ClassSym`, `InterfaceSym`, …) coexist in the same struct; unused
//! collections simply stay empty.
//!
//! This allows a single disk cache format, unified `ParseSnapshot`, and
//! lays the groundwork for multi-language files (e.g. JASS strings in AS).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use url::Url;

// ─── Language marker ─────────────────────────────────────────────────────────

/// Which language produced these symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Lang {
    #[default]
    Jass,
    As,
}

// ─── Parameter ──────────────────────────────────────────────────────────────

/// A function/method parameter: `type name`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSym {
    pub name: String,
    pub type_name: String,
}

// ─── Function / method ──────────────────────────────────────────────────────

/// A function or method declaration (free function, native, or AS method).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FunctionSym {
    pub name: String,
    pub params: Vec<ParamSym>,
    pub return_type: Option<String>,
    /// Enclosing namespace (AS); empty for JASS / top-level.
    #[serde(default)]
    pub namespace: String,
    /// `start_byte` of the declaring node — used as `decl_key` (AS).
    #[serde(default)]
    pub decl_byte: usize,
    /// `//*` doc comment (markdown).
    pub doc_comment: Option<String>,

    // ── JASS-specific ──────────────────────────────────────────────
    /// `true` for `constant native` / `constant function`.
    #[serde(default)]
    pub is_constant: bool,
    /// Declaration order inside the file (0-based).
    #[serde(default)]
    pub decl_index: usize,
    /// Names of functions directly called from the body.
    #[serde(default)]
    pub callees: HashSet<String>,
    /// Diagnostic suppression tags.
    #[serde(default)]
    pub ignore_tags: HashSet<String>,
    /// Single-return inline candidate.
    #[serde(default)]
    pub is_single_return: bool,
    /// Flattened return expression text (when `is_single_return`).
    #[serde(default)]
    pub inline_return_text: Option<String>,
    /// Whether the return expr needs parens when inlined.
    #[serde(default)]
    pub inline_is_compound: bool,
}

// ─── Native ─────────────────────────────────────────────────────────────────

/// A JASS `native` declaration — callable but has no body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeSym {
    pub name: String,
    pub params: Vec<ParamSym>,
    pub return_type: Option<String>,
    #[serde(default)]
    pub is_constant: bool,
    #[serde(default)]
    pub decl_index: usize,
    pub doc_comment: Option<String>,
    #[serde(default)]
    pub ignore_tags: HashSet<String>,
}

// ─── Type (JASS) ────────────────────────────────────────────────────────────

/// A JASS `type X extends Y` declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeSym {
    pub name: String,
    pub base: Option<String>,
    #[serde(default)]
    pub decl_index: usize,
    pub doc_comment: Option<String>,
    #[serde(default)]
    pub ignore_tags: HashSet<String>,
}

// ─── Global variable ────────────────────────────────────────────────────────

/// A global variable (JASS `globals` block or AS top-level/namespace).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalVarSym {
    pub name: String,
    pub type_name: Option<String>,
    /// Enclosing namespace (AS); empty for JASS / top-level.
    #[serde(default)]
    pub namespace: String,
    /// `start_byte` (AS).
    #[serde(default)]
    pub decl_byte: usize,
    pub doc_comment: Option<String>,

    // ── JASS-specific ──────────────────────────────────────────────
    #[serde(default)]
    pub is_constant: bool,
    #[serde(default)]
    pub is_array: bool,
    #[serde(default)]
    pub has_initializer: bool,
    #[serde(default)]
    pub decl_index: usize,
    #[serde(default)]
    pub ignore_tags: HashSet<String>,
}

// ─── Class member (AS) ──────────────────────────────────────────────────────

/// A method inside a class / interface / mixin.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MethodSym {
    pub name: String,
    pub params: Vec<ParamSym>,
    pub return_type: Option<String>,
    pub doc_comment: Option<String>,
    #[serde(default)]
    pub decl_byte: usize,
}

/// A property (field) inside a class / mixin.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PropertySym {
    pub name: String,
    pub type_name: Option<String>,
    pub doc_comment: Option<String>,
    #[serde(default)]
    pub decl_byte: usize,
}

// ─── Class (AS) ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassSym {
    pub name: String,
    #[serde(default)]
    pub namespace: String,
    pub doc_comment: Option<String>,
    #[serde(default)]
    pub decl_byte: usize,
    /// Class methods.
    #[serde(default)]
    pub methods: Vec<MethodSym>,
    /// Class properties (fields).
    #[serde(default)]
    pub properties: Vec<PropertySym>,
}

// ─── Interface (AS) ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceSym {
    pub name: String,
    #[serde(default)]
    pub namespace: String,
    pub doc_comment: Option<String>,
    #[serde(default)]
    pub decl_byte: usize,
    /// Interface methods.
    #[serde(default)]
    pub methods: Vec<MethodSym>,
}

// ─── Enum (AS) ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumSym {
    pub name: String,
    #[serde(default)]
    pub namespace: String,
    pub doc_comment: Option<String>,
    #[serde(default)]
    pub decl_byte: usize,
    #[serde(default)]
    pub members: Vec<String>,
}

// ─── Mixin (AS) ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixinSym {
    pub name: String,
    #[serde(default)]
    pub namespace: String,
    pub doc_comment: Option<String>,
    #[serde(default)]
    pub decl_byte: usize,
    #[serde(default)]
    pub methods: Vec<MethodSym>,
    #[serde(default)]
    pub properties: Vec<PropertySym>,
}

// ─── Typedef (AS) ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedefSym {
    pub alias: String,
    pub original: String,
    #[serde(default)]
    pub namespace: String,
    pub doc_comment: Option<String>,
    #[serde(default)]
    pub decl_byte: usize,
}

// ─── Funcdef (AS) ───────────────────────────────────────────────────────────

/// A `funcdef` declaration (delegate/callback signature).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuncdefSym {
    pub name: String,
    pub params: Vec<ParamSym>,
    pub return_type: Option<String>,
    #[serde(default)]
    pub namespace: String,
    pub doc_comment: Option<String>,
    #[serde(default)]
    pub decl_byte: usize,
}

// ─── Namespace (AS) ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceSym {
    pub name: String,
    #[serde(default)]
    pub decl_byte: usize,
}

// ─── Unified file-level symbol table ────────────────────────────────────────

/// All symbols declared in a single source file.
///
/// Works for both JASS (`.j`) and AngelScript (`.as`) — unused collections
/// simply remain empty.  The `lang` field tells which language produced them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileSymbols {
    /// Which language produced these symbols.
    #[serde(default)]
    pub lang: Lang,

    // ── Shared / JASS-origin ────────────────────────────────────────
    pub types: Vec<TypeSym>,
    pub natives: Vec<NativeSym>,
    pub functions: Vec<FunctionSym>,
    pub globals: Vec<GlobalVarSym>,

    // ── AS-only ─────────────────────────────────────────────────────
    #[serde(default)]
    pub classes: Vec<ClassSym>,
    #[serde(default)]
    pub interfaces: Vec<InterfaceSym>,
    #[serde(default)]
    pub enums: Vec<EnumSym>,
    #[serde(default)]
    pub mixins: Vec<MixinSym>,
    #[serde(default)]
    pub typedefs: Vec<TypedefSym>,
    #[serde(default)]
    pub funcdefs: Vec<FuncdefSym>,
    #[serde(default)]
    pub namespaces: Vec<NamespaceSym>,

    // ── Metadata ────────────────────────────────────────────────────
    /// URLs of files imported via `//import!` (frozen / read-only).
    #[serde(default)]
    pub frozen_imports: HashSet<Url>,
    /// Per-file settings parsed from `//set key value` directives.
    #[serde(default)]
    pub file_settings: HashMap<String, String>,
    /// File-level diagnostic suppression tags.
    #[serde(default)]
    pub file_ignore_tags: HashSet<String>,
    /// Function names called from bare top-level statements.
    #[serde(default)]
    pub bare_callees: HashSet<String>,
    /// `true` when the file contains a `//entry` directive.
    #[serde(default)]
    pub is_entry: bool,
    /// Function names used via `function NAME` references (JASS).
    #[serde(default)]
    pub func_refs: HashSet<String>,
}

#[allow(dead_code)]
impl FileSymbols {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty JASS symbol table.
    pub fn new_jass() -> Self {
        Self { lang: Lang::Jass, ..Default::default() }
    }

    /// Create an empty AS symbol table.
    pub fn new_as() -> Self {
        Self { lang: Lang::As, ..Default::default() }
    }

    /// Find a function by name.
    pub fn find_function(&self, name: &str) -> Option<&FunctionSym> {
        self.functions.iter().find(|f| f.name == name)
    }

    /// Find a native by name.
    pub fn find_native(&self, name: &str) -> Option<&NativeSym> {
        self.natives.iter().find(|n| n.name == name)
    }

    /// Find any callable (function or native) by name.
    pub fn find_callable(&self, name: &str) -> Option<CallableRef<'_>> {
        if let Some(f) = self.find_function(name) {
            Some(CallableRef::Function(f))
        } else {
            self.find_native(name).map(CallableRef::Native)
        }
    }

    /// Find a JASS type by name.
    pub fn find_type(&self, name: &str) -> Option<&TypeSym> {
        self.types.iter().find(|t| t.name == name)
    }

    /// Find a global variable by name.
    pub fn find_global(&self, name: &str) -> Option<&GlobalVarSym> {
        self.globals.iter().find(|g| g.name == name)
    }

    /// Check if *any* symbol with `name` is declared in this file.
    pub fn has_symbol(&self, name: &str) -> bool {
        self.functions.iter().any(|f| f.name == name)
            || self.natives.iter().any(|n| n.name == name)
            || self.globals.iter().any(|g| g.name == name)
            || self.types.iter().any(|t| t.name == name)
            || self.classes.iter().any(|c| c.name == name)
            || self.interfaces.iter().any(|i| i.name == name)
            || self.enums.iter().any(|e| e.name == name)
            || self.mixins.iter().any(|m| m.name == name)
            || self.typedefs.iter().any(|t| t.alias == name)
            || self.funcdefs.iter().any(|f| f.name == name)
    }
}

/// A reference to either a function or a native.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum CallableRef<'a> {
    Function(&'a FunctionSym),
    Native(&'a NativeSym),
}

