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

mod array_cast;
mod convert;
mod emit;
mod fold_ir;
mod inline;
mod io;
mod ir;
mod map_data;
mod null_to_nil;
mod render_as;
mod render_jass;
mod uglify_ir;

use crate::util::file_store::FILE_STORE;
use crate::util::import_graph::IMPORT_GRAPH;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use url::Url;

use self::convert::{collect_ir, resolve_frozen_deps};
use self::inline::{apply_inlines, fold_string_hash_in_fragments};
use self::io::{
    collect_file_order, emit_frozen_import_header, is_archive_path, resolve_output_path,
    write_output, write_output_archive,
};
use self::ir::*;
use self::map_data::{augment_config, augment_main, read_map_data};
use self::render_jass::{hoist_ir_locals, render_jass_function, render_jass_stmt};

// Re-export test-only wrappers so `build_test.rs` can use `use crate::lng::jass::build::*`.
#[cfg(test)]
pub use self::inline::{
    detect_inline_candidate_text, inline_call_in_source_text, is_top_level_call_text,
};

/// Test-only: parse JASS source → AST → IR → render JASS text → hoist locals.
///
/// This mirrors the real `build_jass` pipeline for a single function.
#[cfg(test)]
pub fn build_single_function_jass(src: &str) -> String {
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

    // AST → IR → hoist locals → JASS text
    let mut ir_func = convert::convert_function(src, func, HashSet::new());
    render_jass::hoist_ir_locals(&mut ir_func.body);
    let empty_map = HashMap::new();
    render_jass::render_jass_function(&ir_func, &empty_map)
}

/// Test-only: parse JASS source → AST → IR → render AS text.
///
/// This mirrors the real `build_as` pipeline for a single function,
/// rendering directly from IR via `render_as_function`.
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

    // Hoist late locals to the top (before semantic passes).
    render_jass::hoist_ir_locals(&mut ir_func.body);

    // Rewrite null → nil for handle-typed contexts.
    null_to_nil::rewrite_func_null_to_nil(&mut ir_func);

    // Wrap array reads with type casts (table is untyped in AS).
    array_cast::insert_array_casts_func(&mut ir_func);

    // IR → AS text (direct, no JASS intermediate)
    let rename_map = std::collections::HashMap::new();
    render_as::render_as_function(&ir_func, &rename_map)
}

/// Test-only: parse a JASS global var declaration → AST → IR → render JASS → convert to AS.
#[cfg(test)]
pub fn build_global_var_as(src: &str) -> String {
    use crate::lng::jass::ast::{build_ast, rewrite_imports, Statement};

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_jass::language().into())
        .expect("Failed to set language");
    let tree = parser.parse(src, None).expect("Failed to parse");
    let mut ast = build_ast(tree.root_node());
    let src_bytes = src.as_bytes().to_vec();
    rewrite_imports(&mut ast, &src_bytes);

    let var = ast
        .items
        .iter()
        .find_map(|item| match item {
            Statement::VarStmt(v) => Some(v),
            _ => None,
        })
        .expect("No VarStmt found");

    // AST → IR
    let ir_stmt = convert::convert_stmt(src, &Statement::VarStmt(var.clone()))
        .expect("Failed to convert VarStmt");

    // IR → AS text (direct, no JASS intermediate)
    let rename_map = std::collections::HashMap::new();
    let lines = render_as::render_as_stmt(&ir_stmt, "", &rename_map);
    lines.join("\n")
}

/// Test-only: parse a JASS global var declaration → AST → IR → render JASS text.
///
/// Returns the JASS rendering of the global variable declaration.
#[cfg(test)]
pub fn build_global_var_jass(src: &str) -> String {
    use crate::lng::jass::ast::{build_ast, rewrite_imports, Statement};

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_jass::language().into())
        .expect("Failed to set language");
    let tree = parser.parse(src, None).expect("Failed to parse");
    let mut ast = build_ast(tree.root_node());
    let src_bytes = src.as_bytes().to_vec();
    rewrite_imports(&mut ast, &src_bytes);

    let var = ast
        .items
        .iter()
        .find_map(|item| match item {
            Statement::VarStmt(v) => Some(v),
            _ => None,
        })
        .expect("No VarStmt found");

    // AST → IR
    let ir_stmt = convert::convert_stmt(src, &Statement::VarStmt(var.clone()))
        .expect("Failed to convert VarStmt");

    // IR → JASS text
    let empty_map = HashMap::new();
    let jass_lines = render_jass::render_jass_stmt(&ir_stmt, "", &empty_map);
    jass_lines.join("\n")
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
            short_name: None,
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
            short_name: None,
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

    // 7b. Hoist late locals to the top of each function body (IR level).
    for func in ir.functions.values_mut() {
        hoist_ir_locals(&mut func.body);
    }

    // 7c. Fold StringHash(…) → integer and ExecuteFunc(…) → direct call (IR level).
    fold_ir::fold_ir(&mut ir);

    // 7d. Uglify identifiers (if build-uglify is set).
    let uglify_mode = find_build_setting(uri, "build-uglify")
        .map(|(_, v)| v == "1")
        .unwrap_or(false);
    uglify_ir::uglify_ir(&mut ir, uglify_mode, false);

    // 7e. Build global rename map from IR declarations.
    let global_map = uglify_ir::build_global_rename_map(&ir);

    // 8. Render each function to text for inlining / StringHash passes.
    let mut fragments = Fragments {
        globals_out: ir.globals.iter().flat_map(|g| render_jass_stmt(g, "", &global_map)).collect(),
        functions: ir.functions.iter().map(|(name, func)| {
            let source = render_jass_function(func, &global_map);
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

    // Frozen import directives (//import! / //import-ujapi!) with paths
    // relative to the output file.
    let out_dir = out_path.parent().unwrap_or(Path::new("."));
    out.push_str(&emit_frozen_import_header(&ir.frozen_import_directives, out_dir));

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
            out.push_str(&frag.source);
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
            short_name: None,
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
            short_name: None,
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

    // 7c. Hoist late locals (before semantic passes so hoisted defaults
    //     are processed by null→nil and array-cast too).
    for func in ir.functions.values_mut() {
        hoist_ir_locals(&mut func.body);
    }

    // 7d. Rewrite `null` → `nil` for handle-typed contexts.
    null_to_nil::rewrite_null_to_nil(&mut ir);

    // 7e. Wrap array reads with type casts (`table` is untyped in AS).
    array_cast::insert_array_casts(&mut ir);

    // 7f. Fold StringHash(…) → integer and ExecuteFunc(…) → direct call (IR level).
    fold_ir::fold_ir(&mut ir);

    // 7g. Uglify identifiers / resolve AS keyword conflicts.
    let uglify_mode = find_build_setting(uri, "build-uglify")
        .map(|(_, v)| v == "1")
        .unwrap_or(false);
    uglify_ir::uglify_ir(&mut ir, uglify_mode, true);

    // 7h. Build global rename map from IR declarations.
    let mut global_map = uglify_ir::build_global_rename_map(&ir);

    // 8. Add Jass:: namespace prefix for all native function names.
    for name in &ir.native_names {
        global_map.insert(name.clone(), format!("Jass::{}", name));
    }

    // 9. Render to text for inlining / StringHash passes.
    let empty_map = HashMap::new();
    let mut fragments = Fragments {
        globals_out: ir.globals.iter().flat_map(|g| render_jass_stmt(g, "", &empty_map)).collect(),
        functions: ir.functions.iter().map(|(name, func)| {
            let source = render_jass_function(func, &empty_map);
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

    // Frozen import directives (//import! / //import-ujapi!) with paths
    // relative to the output file.
    let out_dir = out_path.parent().unwrap_or(Path::new("."));
    out.push_str(&emit_frozen_import_header(&ir.frozen_import_directives, out_dir));

    // Globals → top-level variable declarations (rendered from IR).
    for g in &ir.globals {
        for line in render_as::render_as_stmt(g, "", &global_map) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    if !ir.globals.is_empty() {
        out.push('\n');
    }

    // Functions in sorted order, rendered to AS from IR.
    for fname in &sorted_funcs {
        if let Some(func) = ir.functions.get(fname) {
            out.push_str(&render_as::render_as_function(func, &global_map));
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
    // Search the visible component (entry-point-aware).
    let component = IMPORT_GRAPH.visible_component(uri);
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
