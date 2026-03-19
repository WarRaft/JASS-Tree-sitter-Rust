use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use url::Url;


// ─── Owned (lifetime-free) symbol types ──────────────────────────────────────

/// A parameter: `type name`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSym {
    pub name: String,
    pub type_name: String,
}

/// A `type X extends Y` declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeSym {
    pub name: String,
    pub base: Option<String>,
    /// Declaration order inside the file (0-based, across all top-level items).
    pub decl_index: usize,
    /// `//*` doc comment (markdown) attached to this declaration.
    pub doc_comment: Option<String>,
    /// Diagnostic tags suppressed via `//@ignore tag1 tag2` above this declaration.
    #[serde(default)]
    pub ignore_tags: HashSet<String>,
}

/// A `native` declaration — callable but has no body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeSym {
    pub name: String,
    pub params: Vec<ParamSym>,
    pub return_type: Option<String>,
    pub is_constant: bool,
    /// Declaration order inside the file.
    pub decl_index: usize,
    /// `//*` doc comment (markdown) attached to this declaration.
    pub doc_comment: Option<String>,
    /// Diagnostic tags suppressed via `//@ignore tag1 tag2` above this declaration.
    #[serde(default)]
    pub ignore_tags: HashSet<String>,
}

/// A `function … endfunction` declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSym {
    pub name: String,
    pub params: Vec<ParamSym>,
    pub return_type: Option<String>,
    pub is_constant: bool,
    /// Declaration order inside the file (0-based, across all top-level items).
    pub decl_index: usize,
    /// Names of functions directly called from the body (including `call`
    /// statements, expressions like `foo(…)`, and `function foo` references).
    /// Used for topological sorting and reachability analysis.
    pub callees: HashSet<String>,
    /// `//*` doc comment (markdown) attached to this declaration.
    pub doc_comment: Option<String>,
    /// Diagnostic tags suppressed via `//@ignore tag1 tag2` above this declaration.
    #[serde(default)]
    pub ignore_tags: HashSet<String>,
    /// `true` when the function takes nothing and the body consists of a
    /// single `return <expr>` statement — an inline candidate.
    #[serde(default)]
    pub is_single_return: bool,
    /// The flattened text of the return expression (only set when
    /// `is_single_return` is true).
    #[serde(default)]
    pub inline_return_text: Option<String>,
    /// Whether the return expression is compound (binary/unary) and needs
    /// parentheses when inlined into a sub-expression.
    #[serde(default)]
    pub inline_is_compound: bool,
}

/// A single global variable declaration (one name inside a `globals` block).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalVarSym {
    pub name: String,
    pub type_name: Option<String>,
    pub is_constant: bool,
    pub is_array: bool,
    pub has_initializer: bool,
    /// Declaration order inside the file.
    pub decl_index: usize,
    /// `//*` doc comment (markdown) attached to this declaration.
    pub doc_comment: Option<String>,
    /// Diagnostic tags suppressed via `//@ignore tag1 tag2` above this declaration.
    #[serde(default)]
    pub ignore_tags: HashSet<String>,
}

// ─── File-level symbol table ─────────────────────────────────────────────────

/// All symbols declared in a single `.j` file, in declaration order.
///
/// This struct is **owned** (no lifetime parameters) so it can live in a
/// `DashMap` that survives the tree-sitter parse tree.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileSymbols {
    pub types: Vec<TypeSym>,
    pub natives: Vec<NativeSym>,
    pub functions: Vec<FunctionSym>,
    pub globals: Vec<GlobalVarSym>,
    /// URLs of files imported via `//import!` (frozen / read-only).
    /// When a file is imported both via `//import` and `//import!`,
    /// the frozen flag wins.
    pub frozen_imports: HashSet<Url>,
    /// Per-file settings parsed from `//set key value` directives.
    pub file_settings: HashMap<String, String>,
    /// File-level diagnostic suppression tags from `//ignore tag` directives.
    pub file_ignore_tags: HashSet<String>,
    /// Function names called from bare top-level statements (outside any
    /// function body).  These are effectively callees of the implicit `main`.
    pub bare_callees: HashSet<String>,
    /// `true` when the file contains a `//entry` directive —
    /// marks it as a build entry point for tree-shaking and import graph traversal.
    #[serde(default)]
    pub is_entry: bool,
    /// Function names used via `function NAME` references (code-type arguments).
    /// These are NOT call sites and cannot be inlined.
    #[serde(default)]
    pub func_refs: HashSet<String>,
}

#[allow(dead_code)]
impl FileSymbols {
    pub fn new() -> Self {
        Self::default()
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

    /// Find a global variable by name.
    pub fn find_global(&self, name: &str) -> Option<&GlobalVarSym> {
        self.globals.iter().find(|g| g.name == name)
    }

    /// Find a type by name.
    pub fn find_type(&self, name: &str) -> Option<&TypeSym> {
        self.types.iter().find(|t| t.name == name)
    }

    /// Check if *any* symbol with `name` is declared in this file.
    pub fn has_symbol(&self, name: &str) -> bool {
        self.functions.iter().any(|f| f.name == name)
            || self.natives.iter().any(|n| n.name == name)
            || self.globals.iter().any(|g| g.name == name)
            || self.types.iter().any(|t| t.name == name)
    }
}

/// A reference to either a function or a native — returned by
/// [`FileSymbols::find_callable`].
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum CallableRef<'a> {
    Function(&'a FunctionSym),
    Native(&'a NativeSym),
}

// ─── Uglify name generator ──────────────────────────────────────────────────

/// Generates short identifiers: `a`, `b`, …, `z`, `A`, …, `Z`,
/// `aa`, `ab`, …  Skips JASS reserved words.
#[derive(Debug, Clone)]
pub struct ShortNameGen {
    counter: usize,
}

const JASS_RESERVED: &[&str] = &[
    "and", "array", "call", "constant", "debug", "else", "elseif",
    "endfunction", "endglobals", "endif", "endloop", "extends", "false",
    "function", "globals", "if", "local", "loop", "native", "not",
    "nothing", "null", "or", "return", "returns", "set", "takes", "then",
    "true", "type",
];

#[allow(dead_code)]
impl ShortNameGen {
    pub fn new() -> Self {
        Self { counter: 0 }
    }

    /// Return the next short identifier, skipping reserved words.
    pub fn next(&mut self) -> String {
        loop {
            let name = Self::encode(self.counter);
            self.counter += 1;
            if !JASS_RESERVED.contains(&name.as_str()) {
                return name;
            }
        }
    }

    /// Encode a number as a base-52 identifier (a-z, A-Z for the first char,
    /// then a-z, A-Z, 0-9 for subsequent chars).
    fn encode(mut n: usize) -> String {
        const FIRST: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        const REST: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

        let first = FIRST[n % FIRST.len()] as char;
        n /= FIRST.len();

        if n == 0 {
            return first.to_string();
        }

        let mut s = String::new();
        s.push(first);
        while n > 0 {
            n -= 1; // shift to 0-based for REST
            s.push(REST[n % REST.len()] as char);
            n /= REST.len();
        }
        s
    }
}

