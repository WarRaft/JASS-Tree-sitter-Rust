//! IR pass: uglify / minify identifiers.
//!
//! Two modes:
//! - **uglify = false** (mode 0): only resolve AS keyword conflicts for AS
//!   builds (append numeric suffix until unique).  JASS builds are a no-op.
//! - **uglify = true** (mode 1): rename ALL non-frozen identifiers to short
//!   generated names (`a`, `b`, … `aa`, `ab`, …).  Both JASS and AS keywords
//!   are avoided so the output is valid for either target.
//!
//! Frozen identifiers (native function names, `main`, `config`) are never
//! renamed.
//!
//! JASS shadowing is handled correctly: each declaration position (parameter,
//! local) receives its own unique `short_name`.  The last declaration with a
//! given original name is the one visible in the function body — the scope
//! map built by `build_func_scope_map` takes care of this at render time.

use std::collections::{HashMap, HashSet};

use super::ir::*;
use super::render_as::AS_RESERVED;

// ─── Reserved-word sets ──────────────────────────────────────────────────────

const JASS_RESERVED: &[&str] = &[
    "and", "array", "call", "constant", "debug", "else", "elseif",
    "endfunction", "endglobals", "endif", "endloop", "extends", "false",
    "function", "globals", "if", "local", "loop", "native", "not",
    "nothing", "null", "or", "return", "returns", "set", "takes", "then",
    "true", "type",
];

// ─── Short name generator ────────────────────────────────────────────────────

/// Generates short identifiers `a`, `b`, …, `Z`, `aa`, `ab`, …
/// Skips any name in `forbidden` (reserved words + frozen identifiers).
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
                return name;
            }
        }
    }

    /// Encode a number as a compact identifier.
    /// First char: `[a-zA-Z]` (52 choices).
    /// Subsequent chars: `[a-zA-Z0-9]` (62 choices).
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
            n -= 1;
            s.push(REST[n % REST.len()] as char);
            n /= REST.len();
        }
        s
    }
}

// ─── Build the global rename map from IR declarations ────────────────────────

/// Collect all `short_name` mappings from IR declarations into a flat map.
///
/// This map covers function names and global variable names.  Per-function
/// scope (params + locals with shadowing) is handled separately by
/// `build_func_scope_map` at render time.
pub(super) fn build_global_rename_map(ir: &BuildIR) -> HashMap<String, String> {
    let mut map = HashMap::new();

    // Function names.
    for func in ir.functions.values() {
        if let Some(ref sn) = func.short_name {
            map.insert(func.name.clone(), sn.clone());
        }
    }

    // Global variable names.
    for g in &ir.globals {
        if let IRStmt::VarDecl { decls, .. } = g {
            for d in decls {
                if let Some(ref sn) = d.short_name {
                    map.insert(d.name.clone(), sn.clone());
                }
            }
        }
    }

    map
}

// ─── Public entry point ──────────────────────────────────────────────────────

/// Assign `short_name` to IR declarations.
///
/// - `uglify = false, for_as = false` → no-op (JASS mode 0).
/// - `uglify = false, for_as = true`  → rename only AS keyword conflicts.
/// - `uglify = true`                  → rename ALL non-frozen identifiers.
pub(super) fn uglify_ir(ir: &mut BuildIR, uglify: bool, for_as: bool) {
    if uglify {
        uglify_full(ir);
    } else if for_as {
        uglify_as_keywords(ir);
    }
    // JASS mode 0 → nothing to do.
}

// ─── Mode 0: AS keyword conflict resolution ─────────────────────────────────

/// Resolve AS reserved-word conflicts by appending a numeric suffix.
///
/// For each function name or global variable name that collides with an AS
/// keyword, generate `name1`, `name2`, … until no collision with existing
/// names or reserved words.
fn uglify_as_keywords(ir: &mut BuildIR) {
    let reserved: HashSet<&str> = AS_RESERVED.iter().copied().collect();

    // Collect all existing names to avoid collisions.
    let mut all_names: HashSet<String> = HashSet::new();
    for name in ir.functions.keys() {
        all_names.insert(name.clone());
    }
    for g in &ir.globals {
        if let IRStmt::VarDecl { decls, .. } = g {
            for d in decls {
                all_names.insert(d.name.clone());
            }
        }
    }
    // Also params and locals.
    for func in ir.functions.values() {
        for p in &func.params {
            all_names.insert(p.param_name.clone());
        }
        for stmt in &func.body {
            if let IRStmt::Local { name, .. } = stmt {
                all_names.insert(name.clone());
            }
        }
    }

    // Helper: find a non-colliding suffixed name.
    let find_unique = |base: &str, all: &HashSet<String>| -> String {
        let mut suffix = 1u32;
        loop {
            let candidate = format!("{}{}", base, suffix);
            if !reserved.contains(candidate.as_str()) && !all.contains(&candidate) {
                return candidate;
            }
            suffix += 1;
        }
    };

    // Rename function names.
    let func_names: Vec<String> = ir.functions.keys().cloned().collect();
    for fname in &func_names {
        if reserved.contains(fname.as_str()) {
            let new_name = find_unique(fname, &all_names);
            all_names.insert(new_name.clone());
            if let Some(func) = ir.functions.get_mut(fname) {
                func.short_name = Some(new_name);
            }
        }
    }

    // Rename global variables.
    for g in ir.globals.iter_mut() {
        if let IRStmt::VarDecl { decls, .. } = g {
            for d in decls.iter_mut() {
                if reserved.contains(d.name.as_str()) {
                    let new_name = find_unique(&d.name, &all_names);
                    all_names.insert(new_name.clone());
                    d.short_name = Some(new_name);
                }
            }
        }
    }

    // Rename function parameters and locals.
    for func in ir.functions.values_mut() {
        for p in func.params.iter_mut() {
            if reserved.contains(p.param_name.as_str()) {
                let new_name = find_unique(&p.param_name, &all_names);
                all_names.insert(new_name.clone());
                p.short_name = Some(new_name);
            }
        }
        for stmt in func.body.iter_mut() {
            if let IRStmt::Local { name, short_name, .. } = stmt {
                if reserved.contains(name.as_str()) {
                    let new_name = find_unique(name, &all_names);
                    all_names.insert(new_name.clone());
                    *short_name = Some(new_name);
                }
            }
        }
    }
}

// ─── Mode 1: full uglification ───────────────────────────────────────────────

/// Rename all non-frozen identifiers to short generated names.
fn uglify_full(ir: &mut BuildIR) {
    // Build forbidden set: JASS reserved + AS reserved + frozen names.
    let mut forbidden: HashSet<String> = HashSet::new();
    for &w in JASS_RESERVED {
        forbidden.insert(w.to_string());
    }
    for &w in AS_RESERVED {
        forbidden.insert(w.to_string());
    }

    // Frozen identifiers: native names + entry points.
    let mut frozen_names: HashSet<String> = HashSet::new();
    for name in &ir.native_names {
        frozen_names.insert(name.clone());
        forbidden.insert(name.clone());
    }
    // main and config are entry points — never renamed.
    frozen_names.insert("main".to_string());
    frozen_names.insert("config".to_string());
    forbidden.insert("main".to_string());
    forbidden.insert("config".to_string());

    let mut name_gen = NameGen::new(forbidden);

    // 1. Rename function names (non-frozen).
    let func_names: Vec<String> = ir.functions.keys().cloned().collect();
    for fname in &func_names {
        if frozen_names.contains(fname) {
            continue;
        }
        let short = name_gen.next();
        if let Some(func) = ir.functions.get_mut(fname) {
            func.short_name = Some(short);
        }
    }

    // 2. Rename global variables.
    for g in ir.globals.iter_mut() {
        if let IRStmt::VarDecl { decls, .. } = g {
            for d in decls.iter_mut() {
                if frozen_names.contains(&d.name) {
                    continue;
                }
                d.short_name = Some(name_gen.next());
            }
        }
    }

    // 3. Rename function parameters and locals.
    //    Each declaration gets its own unique short_name (handles shadowing).
    for func in ir.functions.values_mut() {
        for p in func.params.iter_mut() {
            p.short_name = Some(name_gen.next());
        }
        for stmt in func.body.iter_mut() {
            if let IRStmt::Local { short_name, .. } = stmt {
                *short_name = Some(name_gen.next());
            }
        }
    }
}

