//! JASS build — merge the import tree into a single `.j` or `.as` file.
//!
//! Searches the entire connected component of the import tree for
//! `//set build-jass <path>` / `//set build-as <path>` directives.
//!
//! **Frozen files (`//import!`)** handling depends on the build mode:
//! - **JASS**: frozen files are excluded from the build entirely —
//!   they are engine-provided / read-only.
//! - **AS**: frozen files contribute their functions and global variables
//!   to the output (types and natives are still skipped).  Native function
//!   calls are prefixed with the `Jass::` namespace.
//!
//! **`type` and `native` declarations** are never included in the build
//! output — they are engine-provided and do not belong in the merged file.
//!
//! Output structure (JASS):
//! 1. `globals … endglobals` (merged from all files)
//! 2. Functions in topological order (callee above caller)
//! 3. Bare top-level statements → synthesized `function main takes nothing returns nothing … endfunction`
//!
//! When the build target is a `.w3x` / `.w3m` archive the map's
//! `war3map.w3i` and `war3mapUnits.doo` are read first and the
//! player slot setup, team configuration, unit/item/destructable
//! creation, camera/DNC/sound setup are all generated directly into
//! `config()` and `main()` — no intermediate helper functions are emitted.
//!
//! Output structure (AngelScript):
//! 1. Global variable declarations
//! 2. Functions in topological order
//! 3. Bare top-level statements → synthesized `void main() { … }`

mod convert;
mod emit;
mod inline;
mod io;
mod ir;
mod jass_to_as;
mod map_data;
mod null_to_nil;
mod render_as;
mod render_jass;

use crate::util::file_store::FILE_STORE;
use crate::util::import_graph::IMPORT_GRAPH;
use serde::Serialize;
use std::collections::HashSet;
use url::Url;

use self::convert::{collect_ir, resolve_frozen_deps};
use self::inline::{apply_inlines, fold_string_hash_in_fragments};
use self::io::{
    collect_file_order, is_archive_path, resolve_output_path, write_output, write_output_archive,
};
use self::ir::*;
use self::jass_to_as::{jass_function_to_as, jass_var_decl_to_as};
use self::map_data::{augment_config, augment_main, read_map_data};
use self::render_as::build_as_rename_map;
use self::render_jass::{hoist_jass_locals, render_jass_function, render_jass_stmt};

// Re-export test-only wrappers so `build_test.rs` can use `use crate::lng::jass::build::*`.
#[cfg(test)]
pub use self::emit::{emit_function_text, emit_function_text_as, emit_var_text_as};
#[cfg(test)]
pub use self::inline::{
    detect_inline_candidate_text, inline_call_in_source_text, is_top_level_call_text,
};
#[cfg(test)]
pub use self::render_jass::hoist_jass_locals_text;
#[cfg(test)]
pub use self::jass_to_as::{jass_function_to_as_text, jass_var_decl_to_as_text};

/// Test-only: parse JASS source → AST → IR → render JASS text → convert to AS.
///
/// This mirrors the real `build_as` pipeline for a single function.
#[cfg(test)]
pub fn build_single_function_as(src: &str) -> String {
    use crate::lng::jass::ast::{build_ast, rewrite_imports, Statement};
    use std::collections::HashSet;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_jass::language().into())
        .expect("Failed to set language");
    let tree = parser.parse(src, None).expect("Failed to parse");
    let mut ast = build_ast(tree.root_node());
    let src_bytes = src.as_bytes().to_vec();
    rewrite_imports(&mut ast, &src_bytes);

    let func = ast
        .items
        .iter()
        .find_map(|item| match item {
            Statement::Function(f) => Some(f),
            _ => None,
        })
        .expect("No function found");

    // AST → IR
    let mut ir_func = convert::convert_function(src, func, HashSet::new());

    // Rewrite null → nil for handle-typed contexts.
    null_to_nil::rewrite_func_null_to_nil(&mut ir_func);

    // IR → JASS text (with precedence parenthesization)
    let jass_text = render_jass::render_jass_function(&ir_func);

    // JASS text → AS text (jass_function_to_as does its own AS-level hoisting)
    let rename_map = std::collections::HashMap::new();
    jass_to_as::jass_function_to_as(&jass_text, &rename_map)
}

/// Build mode — determines how frozen (`//import!`) files are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(self) enum BuildMode {
    /// JASS build: frozen files are skipped entirely.
    Jass,
    /// AngelScript build: frozen files contribute only **reachable** functions
    /// (transitively called from user code) and the global variables those
    /// functions use.  Native calls are prefixed with `Jass::`.
    As,
}

// ─── Public API ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct BuildResult {
    /// `true` when the file was successfully written.
    pub ok: bool,
    /// Path where the output was written (empty on error).
    pub path: String,
    /// Human-readable message (success or error description).
    pub message: String,
}

/// Execute the JASS build for the file at `uri`.
///
/// The build always starts from an `//entry` file.  The entry file must
/// carry a `//set build-jass <path>` directive (or one of the entry files
/// reachable in the same connected component must have it).
///
/// Files not reachable from the entry point are excluded (tree-shaking).
/// If no `//entry` file with a `build-jass` setting is found, an error
/// is returned — the build requires an explicit entry point.
pub fn build_jass(uri: &Url) -> BuildResult {
    // 1. Find build target — must be in an //entry file when entries exist.
    let (trigger_uri, target) = match find_build_setting(uri, "build-jass") {
        Some(pair) => pair,
        None => return err(crate::util::i18n::build_no_setting_jass()),
    };

    // 2. Resolve output path relative to the file that owns the directive.
    let base_dir = match trigger_uri.to_file_path() {
        Ok(p) => match p.parent() {
            Some(d) => d.to_path_buf(),
            None => return err(crate::util::i18n::build_no_parent_dir()),
        },
        Err(_) => return err(crate::util::i18n::build_not_file_path()),
    };

    let out_path = resolve_output_path(&base_dir, &target, "war3map.j");
    let archive_mode = is_archive_path(&out_path);

    // 2b. If target is an archive, read w3i + doo data from it.
    let map_data = if archive_mode {
        match read_map_data(&out_path) {
            Ok(md) => Some(md),
            Err(e) => return err(&e),
        }
    } else {
        None
    };

    // 3. Collect ordered file list starting from the entry/trigger.
    let file_order = collect_file_order(&trigger_uri);

    // 4. Parse all files → IR.
    let mut ir = collect_ir(&trigger_uri, &file_order);

    // 5. Ensure main exists; if archive — ensure config too.
    if !ir.functions.contains_key("main") {
        ir.functions.insert("main".into(), IRFunc {
            name: "main".into(),
            params: vec![],
            return_type: "nothing".into(),
            body: vec![],
            callees: HashSet::new(),
            inline_expr: None,
        });
    }
    if archive_mode && !ir.functions.contains_key("config") {
        ir.functions.insert("config".into(), IRFunc {
            name: "config".into(),
            params: vec![],
            return_type: "nothing".into(),
            body: vec![],
            callees: HashSet::new(),
            inline_expr: None,
        });
    }

    // 6. Augment main and config from binary data.
    if let Some(ref md) = map_data {
        augment_config(&mut ir, md);
        augment_main(&mut ir, md);
    }

    // 7. Move bare_stmts into main body (append after generated + user code).
    {
        let bare = std::mem::take(&mut ir.bare_stmts);
        let main_func = ir.functions.get_mut("main").unwrap();
        main_func.body.extend(bare);
    }

    // 8. Render each function to text for inlining / StringHash passes.
    let mut fragments = Fragments {
        globals_out: ir.globals.iter().flat_map(|g| render_jass_stmt(g, "")).collect(),
        functions: ir.functions.iter().map(|(name, func)| {
            let source = render_jass_function(func);
            (name.clone(), FuncFragment {
                name: name.clone(),
                source,
                callees: func.callees.clone(),
                inline_expr: func.inline_expr.clone(),
            })
        }).collect(),
        bare_stmts: vec![],
    };

    // 8b. Inline single-call-site trivial functions.
    apply_inlines(&mut fragments);

    // 8c. Fold StringHash("literal") → integer constant.
    fold_string_hash_in_fragments(&mut fragments);

    // 9. Topological sort — config first, main last.
    let sorted_funcs = topo_sort_ir(&ir.functions);

    // 10. Assemble output.
    let mut out = String::new();

    // Globals.
    if !fragments.globals_out.is_empty() {
        out.push_str("globals\n");
        for g in &fragments.globals_out {
            out.push_str("    ");
            out.push_str(g.trim());
            out.push('\n');
        }
        out.push_str("endglobals\n\n");
    }

    // Functions in sorted order.
    for fname in &sorted_funcs {
        if let Some(frag) = fragments.functions.get(fname) {
            let src = hoist_jass_locals(&frag.source);
            out.push_str(&src);
            out.push_str("\n\n");
        }
    }

    // 11. Write output.
    if archive_mode {
        let backup_setting = find_build_setting(uri, "backup");
        write_output_archive(&out_path, &out, &sorted_funcs, &fragments, "war3map.j", &base_dir, backup_setting.as_ref(), BuildMode::Jass)
    } else {
        write_output(&out_path, &out, &sorted_funcs, &fragments)
    }
}

/// Execute the AngelScript build for the file at `uri`.
///
/// Looks up `build-as` across the entire import tree, resolves the output
/// path relative to the file that contains the directive, collects all
/// JASS sources from the import tree, converts them to AngelScript syntax,
/// and emits a merged `.as` file.
///
/// Identifiers that collide with AngelScript reserved words are renamed
/// by appending a numeric suffix (`name` → `name1`, `name2`, …).
pub fn build_as(uri: &Url) -> BuildResult {
    // 1. Find build target across the whole tree.
    let (trigger_uri, target) = match find_build_setting(uri, "build-as") {
        Some(pair) => pair,
        None => return err(crate::util::i18n::build_no_setting_as()),
    };

    // 2. Resolve output path relative to the file that owns the directive.
    let base_dir = match trigger_uri.to_file_path() {
        Ok(p) => match p.parent() {
            Some(d) => d.to_path_buf(),
            None => return err(crate::util::i18n::build_no_parent_dir()),
        },
        Err(_) => return err(crate::util::i18n::build_not_file_path()),
    };

    let out_path = resolve_output_path(&base_dir, &target, "war3map.as");
    let archive_mode = is_archive_path(&out_path);

    // 2b. If target is an archive, read w3i + doo data from it.
    let map_data = if archive_mode {
        match read_map_data(&out_path) {
            Ok(md) => Some(md),
            Err(e) => return err(&e),
        }
    } else {
        None
    };

    // 3. Collect ordered file list from import tree.
    let file_order = collect_file_order(&trigger_uri);

    // 4. Parse non-frozen files → IR.
    let mut ir = collect_ir(&trigger_uri, &file_order);

    // 5. Ensure main exists; if archive — ensure config too.
    if !ir.functions.contains_key("main") {
        ir.functions.insert("main".into(), IRFunc {
            name: "main".into(),
            params: vec![],
            return_type: "nothing".into(),
            body: vec![],
            callees: HashSet::new(),
            inline_expr: None,
        });
    }
    if archive_mode && !ir.functions.contains_key("config") {
        ir.functions.insert("config".into(), IRFunc {
            name: "config".into(),
            params: vec![],
            return_type: "nothing".into(),
            body: vec![],
            callees: HashSet::new(),
            inline_expr: None,
        });
    }

    // 6. Augment main and config from binary data.
    if let Some(ref md) = map_data {
        augment_config(&mut ir, md);
        augment_main(&mut ir, md);
    }

    // 7. Move bare_stmts into main body.
    {
        let bare = std::mem::take(&mut ir.bare_stmts);
        let main_func = ir.functions.get_mut("main").unwrap();
        main_func.body.extend(bare);
    }

    // 7b. Resolve frozen-file dependencies — AFTER augmentation so that
    //     generated calls (InitBlizzard, SetPlayerAllianceStateBJ, …) are
    //     included in the reachability analysis.
    resolve_frozen_deps(&mut ir, &file_order);

    // 7c. Rewrite `null` → `nil` for handle-typed contexts.
    null_to_nil::rewrite_null_to_nil(&mut ir);

    // 8. Build rename map for AS reserved-word conflicts.
    let mut all_names: Vec<&str> = ir.functions.keys().map(|s| s.as_str()).collect();
    all_names.sort();
    let mut rename_map = build_as_rename_map(&all_names);

    // 8b. Add Jass:: namespace prefix for all native function names.
    // This uses the existing word-boundary replacement in apply_rename_to_line
    // so every occurrence of a native identifier becomes Jass::NativeName.
    for name in &ir.native_names {
        rename_map.insert(name.clone(), format!("Jass::{}", name));
    }

    // 9. Render to text for inlining / StringHash passes.
    // For inlining we render to JASS text (the canonical form),
    // apply text-based passes, then re-render won't happen — inlining
    // operates on the JASS text and then we convert to AS from that.
    let mut fragments = Fragments {
        globals_out: ir.globals.iter().flat_map(|g| render_jass_stmt(g, "")).collect(),
        functions: ir.functions.iter().map(|(name, func)| {
            let source = render_jass_function(func);
            (name.clone(), FuncFragment {
                name: name.clone(),
                source,
                callees: func.callees.clone(),
                inline_expr: func.inline_expr.clone(),
            })
        }).collect(),
        bare_stmts: vec![],
    };

    apply_inlines(&mut fragments);
    fold_string_hash_in_fragments(&mut fragments);

    // 10. Topological sort — config first, main last.
    let sorted_funcs = topo_sort_ir(&ir.functions);

    // 11. Assemble AS output.
    let mut out = String::new();

    // Globals → top-level variable declarations.
    for g in &fragments.globals_out {
        out.push_str(&jass_var_decl_to_as(g.trim(), &rename_map));
        out.push('\n');
    }
    if !fragments.globals_out.is_empty() {
        out.push('\n');
    }

    // Functions in sorted order, converted to AS.
    for fname in &sorted_funcs {
        if let Some(frag) = fragments.functions.get(fname) {
            out.push_str(&jass_function_to_as(&frag.source, &rename_map));
            out.push_str("\n\n");
        }
    }

    if archive_mode {
        let backup_setting = find_build_setting(uri, "backup");
        write_output_archive(&out_path, &out, &sorted_funcs, &fragments, "war3map.as", &base_dir, backup_setting.as_ref(), BuildMode::As)
    } else {
        write_output(&out_path, &out, &sorted_funcs, &fragments)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Search for a build setting `key` in `//entry` files within the connected
/// component of the import tree. Returns `(uri_of_entry_file, setting_value)`.
///
/// Build directives (`//set build-jass`, `//set build-as`) are only honoured
/// in files that carry the `//entry` directive.  Non-entry files that contain
/// these settings are diagnosed as errors and are never used as build origins.
/// The build always starts from the entry file and follows its imports.
fn find_build_setting(uri: &Url, key: &str) -> Option<(Url, String)> {
    // Check the current file first.
    if let Some(fs) = FILE_STORE.get(uri) {
        if fs.file_symbols.is_entry {
            if let Some(v) = fs.file_symbols.file_settings.get(key) {
                return Some((uri.clone(), v.clone()));
            }
        }
    }
    // Search the connected component.
    let component = IMPORT_GRAPH.connected_component(uri);
    for u in &component {
        if let Some(fs) = FILE_STORE.get(u) {
            if fs.file_symbols.is_entry {
                if let Some(v) = fs.file_symbols.file_settings.get(key) {
                    return Some((u.clone(), v.clone()));
                }
            }
        }
    }
    None
}

/// Check whether `key` exists in any file of the connected component.
pub fn has_build_setting(uri: &Url, key: &str) -> bool {
    find_build_setting(uri, key).is_some()
}

fn err(msg: &str) -> BuildResult {
    BuildResult {
        ok: false,
        path: String::new(),
        message: msg.to_string(),
    }
}

