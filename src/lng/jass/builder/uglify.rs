//! Build pass: uglify (minify) all user-defined identifiers in the assembled
//! JASS output.
//!
//! # Algorithm
//! 1. During AST analysis, collect every declared identifier from non-frozen
//!    files (function names, global variable names, parameter names, local
//!    variable names).
//! 2. Build a rename map: assign each collected name a short generated
//!    identifier (`a`, `b`, …, `Z`, `aa`, `ab`, …), skipping JASS reserved
//!    words and a set of frozen names (`main`, `config`).
//! 3. Apply the rename map to the assembled text using a simple tokenizer
//!    that respects string literals, four-char codes, and line comments.

use std::collections::{HashMap, HashSet};

use crate::lng::jass::ast::{Statement, VarStmt};
use crate::lng::jass::builder::render::id_str;

// ─── JASS reserved words ─────────────────────────────────────────────────────

const JASS_RESERVED: &[&str] = &[
    "and", "array", "call", "constant", "debug", "else", "elseif",
    "endfunction", "endglobals", "endif", "endloop", "extends", "false",
    "function", "globals", "if", "local", "loop", "native", "not",
    "nothing", "null", "or", "return", "returns", "set", "takes", "then",
    "true", "type",
];

/// Names that must never be renamed, regardless of uglify mode.
const FROZEN_NAMES: &[&str] = &["main", "config"];

// ─── Short-name generator ─────────────────────────────────────────────────────

struct NameGen {
    counter: usize,
    forbidden: HashSet<String>,
}

impl NameGen {
    fn new(forbidden: HashSet<String>) -> Self {
        Self { counter: 0, forbidden }
    }

    fn next(&mut self) -> String {
        loop {
            let name = Self::encode(self.counter);
            self.counter += 1;
            if !self.forbidden.contains(&name) {
                self.forbidden.insert(name.clone());
                return name;
            }
        }
    }

    /// Encode a number as a compact identifier.
    /// First char: `[a-zA-Z]` (52 choices); subsequent chars: `[a-zA-Z0-9]`.
    fn encode(mut n: usize) -> String {
        const FIRST: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        const REST: &[u8] =
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

        let first = FIRST[n % FIRST.len()] as char;
        n /= FIRST.len();

        if n == 0 {
            return first.to_string();
        }

        let mut s = String::new();
        s.push(first);
        while n > 0 {
            n -= 1;
            s.push(REST[n % REST.len()] as char);
            n /= REST.len();
        }
        s
    }
}

// ─── Name collection ─────────────────────────────────────────────────────────

/// Collect all user-defined identifiers from `stmts` into `out`.
///
/// Recursively descends into function bodies (including `if`/`loop` blocks).
pub fn collect_decl_names(src: &str, stmts: &[Statement<'_>], out: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Statement::Function(f) => {
                if let Some(id) = &f.name {
                    out.push(id_str(src, id).to_string());
                }
                for p in &f.params {
                    if let Some(id) = &p.name {
                        out.push(id_str(src, id).to_string());
                    }
                }
                collect_decl_names(src, &f.body, out);
            }
            Statement::Globals(g) => {
                collect_var_stmt_names(src, &g.vars, out);
            }
            Statement::VarStmt(v) => {
                collect_single_var_names(src, v, out);
            }
            Statement::Local(l) => {
                if let Some(id) = &l.name {
                    out.push(id_str(src, id).to_string());
                }
            }
            Statement::If(s) => {
                collect_decl_names(src, &s.body, out);
                for branch in &s.branches {
                    collect_decl_names(src, &branch.body, out);
                }
            }
            Statement::Loop(s) => {
                collect_decl_names(src, &s.body, out);
            }
            _ => {}
        }
    }
}

fn collect_var_stmt_names(src: &str, vars: &[VarStmt<'_>], out: &mut Vec<String>) {
    for v in vars {
        collect_single_var_names(src, v, out);
    }
}

fn collect_single_var_names(src: &str, v: &VarStmt<'_>, out: &mut Vec<String>) {
    for d in &v.decls {
        if let Some(id) = &d.name {
            out.push(id_str(src, id).to_string());
        }
    }
}

// ─── Rename map ───────────────────────────────────────────────────────────────

/// Build a rename map from `user_names` to short identifiers.
///
/// Names in `FROZEN_NAMES` and names that equal JASS reserved words are left
/// out of the map (they keep their original spelling).
pub fn build_rename_map(user_names: &[String]) -> HashMap<String, String> {
    let mut forbidden: HashSet<String> = JASS_RESERVED.iter().map(|s| s.to_string()).collect();
    for &n in FROZEN_NAMES {
        forbidden.insert(n.to_string());
    }

    let mut name_gen = NameGen::new(forbidden);
    let mut map = HashMap::new();

    // Deduplicate while preserving first-occurrence order.
    let mut seen = HashSet::new();
    for name in user_names {
        if seen.insert(name.clone())
            && !FROZEN_NAMES.contains(&name.as_str())
            && !JASS_RESERVED.contains(&name.as_str())
        {
            map.insert(name.clone(), name_gen.next());
        }
    }

    map
}

// ─── Text-level rename pass ───────────────────────────────────────────────────

/// Apply `rename_map` to `src` by tokenizing, replacing only identifiers.
///
/// String literals (`"…"`), four-char codes (`'xxxx'`), and line comments
/// (`//…`) are passed through verbatim.
pub fn apply_rename(src: &str, rename_map: &HashMap<String, String>) -> String {
    if rename_map.is_empty() {
        return src.to_string();
    }

    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;

    while i < len {
        let b = bytes[i];

        // Line comment — copy until newline.
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            while i < len && bytes[i] != b'\n' {
                out.push(bytes[i] as char);
                i += 1;
            }
            continue;
        }

        // String literal — copy verbatim.
        if b == b'"' {
            out.push('"');
            i += 1;
            while i < len {
                let c = bytes[i];
                out.push(c as char);
                i += 1;
                if c == b'\\' && i < len {
                    out.push(bytes[i] as char);
                    i += 1;
                } else if c == b'"' {
                    break;
                }
            }
            continue;
        }

        // Four-char code — copy verbatim.
        if b == b'\'' {
            out.push('\'');
            i += 1;
            while i < len {
                let c = bytes[i];
                out.push(c as char);
                i += 1;
                if c == b'\'' {
                    break;
                }
            }
            continue;
        }

        // Identifier or keyword.
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let ident = &src[start..i];
            if let Some(short) = rename_map.get(ident) {
                out.push_str(short);
            } else {
                out.push_str(ident);
            }
            continue;
        }

        out.push(b as char);
        i += 1;
    }

    out
}

