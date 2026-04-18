//! Build command: merge all JASS files in the import tree into a single `.j`.
//!
//! Entry point: [`run`].
//!
//! # Algorithm
//! 1. Find `//set build-jass <path>` in the connected component.
//! 2. Collect the ordered file list (dependencies first, entry last).
//! 3. For each non-frozen file: parse source → AST → extract globals/functions/bare-stmts.
//! 4. Wrap bare top-level statements into `function main … endfunction`.
//! 5. Topologically sort functions (callees before callers; `config` first, `main` last).
//! 6. Assemble and write the output file.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use url::Url;

use crate::lng::jass::ast::{annotate_comptime_values, build_ast, rewrite_imports, Statement};
use crate::util::import_graph::resolve_import;

use super::render::{
    render_body_epilogue, render_body_with_hoisting, render_function, render_globals_vars,
    render_var_stmt, HoistRenderState, id_str,
};
use super::project::{collect_project, ProjectAst};
use super::sort::topo_sort;
use super::uglify::{apply_rename, build_rename_map, collect_decl_names};
use crate::lng::jass::builder::{BuildOptions, BuildResult, BuilderReport, PipelineMode};
use crate::lng::jass::builder::collect::{build_opt_tags, find_build_setting};

// ─── Public entry point ───────────────────────────────────────────────────────

/// Execute the JASS builder with explicit pipeline options.
pub fn run_with_options(uri: &Url, options: BuildOptions) -> BuildResult {
    run_report_with_options(uri, options).result
}

/// Execute the JASS builder and return the extended pipeline report.
pub fn run_report_with_options(uri: &Url, options: BuildOptions) -> BuilderReport {
    let project = match collect_project(uri, "build-jass", "war3map.j") {
        Ok(p) => p,
        Err(e) => {
            return BuilderReport {
                result: e,
                diagnostics: Vec::new(),
                files: 0,
                functions: 0,
                globals: 0,
                preview: None,
                applied_fixes: Vec::new(),
            }
        }
    };

    let uglify = find_build_setting(uri, "build-opts")
        .map(|(_, v)| {
            let mut settings = std::collections::HashMap::new();
            settings.insert("build-opts".to_string(), v);
            build_opt_tags(&settings).contains("uglify")
        })
        .unwrap_or(false)
        || find_build_setting(uri, "build-uglify")
            .map(|(_, v)| v == "1")
            .unwrap_or(false);

    let plan = analyze_project(&project, options.mode, uglify);
    let files = project.files.len();
    let functions = plan.sorted_funcs.len();
    let globals = plan.globals.len();

    if options.mode == PipelineMode::Diagnostics {
        return BuilderReport {
            result: BuildResult::ok(
                String::new(),
                format!(
                    "jass diagnostics: {} file(s), {} function(s), {} global(s)",
                    files,
                    functions,
                    globals,
                ),
            ),
            diagnostics: Vec::new(),
            files,
            functions,
            globals,
            preview: None,
            applied_fixes: Vec::new(),
        };
    }

    let out = render_plan(&plan, &project.out_path);

    if !options.write_output {
        return BuilderReport {
            result: BuildResult::ok(
                String::new(),
                format!(
                    "jass build preview: {} function(s), {} global(s)",
                    functions,
                    globals,
                ),
            ),
            diagnostics: Vec::new(),
            files,
            functions,
            globals,
            preview: Some(out),
            applied_fixes: Vec::new(),
        };
    }

    if let Some(parent) = project.out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let result = match std::fs::write(&project.out_path, &out) {
        Ok(_) => BuildResult::ok(
            project.out_path.display().to_string(),
            crate::util::i18n::build_ok(plan.globals.len(), plan.sorted_funcs.len(), plan.bare_stmt_count),
        ),
        Err(e) => BuildResult::err(&crate::util::i18n::build_write_failed(
            &project.out_path.display().to_string(),
            &e.to_string(),
        )),
    };

    BuilderReport {
        result,
        diagnostics: Vec::new(),
        files,
        functions,
        globals,
        preview: None,
        applied_fixes: Vec::new(),
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn parse_jass(src: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_jass::language().into())
        .ok()?;
    parser.parse(src, None)
}

#[derive(Debug, Default, Clone)]
struct JassBuildPlan {
    globals: Vec<String>,
    functions: HashMap<String, String>,
    function_order: Vec<String>,
    function_callees: HashMap<String, HashSet<String>>,
    bare_stmt_count: usize,
    main_state: HoistRenderState,
    main_body_lines: Vec<String>,
    frozen_import_directives: Vec<FrozenImportEntry>,
    sorted_funcs: Vec<String>,
    /// All user-defined identifiers collected from non-frozen files.
    declared_names: Vec<String>,
    uglify: bool,
}

fn analyze_project(project: &ProjectAst, _mode: PipelineMode, uglify: bool) -> JassBuildPlan {
    let mut plan = JassBuildPlan { uglify, ..JassBuildPlan::default() };
    let mut seen_frozen_urls = HashSet::<Url>::new();

    for file in &project.files {
        if file.is_frozen {
            continue;
        }

        for (name, callees) in &file.function_callees {
            plan.function_callees.insert(name.clone(), callees.clone());
        }

        let tree = match parse_jass(&file.source) {
            Some(t) => t,
            None => continue,
        };

        let mut ast = build_ast(tree.root_node());
        rewrite_imports(&mut ast, file.source.as_bytes());
        annotate_comptime_values(&mut ast, file.source.as_bytes());

        if uglify {
            collect_decl_names(&file.source, &ast.items, &mut plan.declared_names);
        }

        for item in &ast.items {
            match item {
                Statement::Type(_) | Statement::Native(_) => {}
                Statement::Import(imp) if imp.frozen => {
                    if !imp.path.is_empty() {
                        if let Some(resolved) = resolve_import(&file.uri, &imp.path) {
                            if seen_frozen_urls.insert(resolved.url.clone()) {
                                plan.frozen_import_directives.push(FrozenImportEntry {
                                    is_ujapi: false,
                                    url: resolved.url,
                                });
                            }
                        }
                    }
                }
                Statement::UjapiImport(ud) => {
                    if !ud.path.is_empty() {
                        if let Some(resolved) = resolve_import(&file.uri, &ud.path) {
                            if seen_frozen_urls.insert(resolved.url.clone()) {
                                plan.frozen_import_directives.push(FrozenImportEntry {
                                    is_ujapi: true,
                                    url: resolved.url,
                                });
                            }
                        }
                    }
                }
                Statement::Globals(g) => {
                    plan.globals.extend(render_globals_vars(&file.source, g));
                }
                Statement::VarStmt(v) => {
                    plan.globals.push(render_var_stmt(&file.source, v));
                }
                Statement::Function(f) => {
                    let name = f
                        .name
                        .as_ref()
                        .map(|id| id_str(&file.source, id).to_string())
                        .unwrap_or_default();
                    if !name.is_empty() {
                        if !plan.functions.contains_key(&name) {
                            plan.function_order.push(name.clone());
                        }
                        let (func_text, extra_globals) = render_function(&file.source, f);
                        plan.globals.extend(extra_globals);
                        plan.functions.insert(name, func_text);
                    }
                }
                other => {
                    let before_len = plan.main_body_lines.len();
                    plan.main_body_lines.extend(render_body_with_hoisting(
                        &file.source,
                        std::slice::from_ref(other),
                        &mut plan.main_state,
                        false,
                    ));
                    if plan.main_body_lines.len() != before_len || matches!(other, Statement::Local(_) | Statement::VarStmt(_)) {
                        plan.bare_stmt_count += 1;
                    }
                }
            }
        }
    }

    if !plan.main_state.hoisted_local_lines.is_empty() || !plan.main_body_lines.is_empty() {
        let main = synthesize_main(&plan.main_state, &plan.main_body_lines);
        if !plan.functions.contains_key("main") {
            plan.function_order.push("main".to_string());
        }
        plan.functions.insert("main".to_string(), main);
    }

    plan.sorted_funcs = topo_sort(&plan.function_order, &plan.functions, &plan.function_callees);
    plan
}

fn render_plan(plan: &JassBuildPlan, out_path: &Path) -> String {
    let mut out = assemble(&plan.globals, &plan.sorted_funcs, &plan.functions);

    let out_dir = out_path.parent().unwrap_or_else(|| Path::new("."));
    let header = emit_frozen_import_header(&plan.frozen_import_directives, out_dir);
    if !header.is_empty() {
        out = format!("{}{}", header, out);
    }

    let out = normalize_output(&out);

    if plan.uglify {
        let rename_map = build_rename_map(&plan.declared_names);
        apply_rename(&out, &rename_map)
    } else {
        out
    }
}

/// Create a new `function main takes nothing returns nothing` from hoisted locals and body lines.
fn synthesize_main(state: &HoistRenderState, body_lines: &[String]) -> String {
    let mut out = String::from("function main takes nothing returns nothing\n");
    let epilogue = render_body_epilogue(state);
    for line in state
        .hoisted_local_lines
        .iter()
        .chain(body_lines.iter())
        .chain(epilogue.iter())
    {
        if line.trim().is_empty() {
            continue;
        }
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("endfunction");
    out
}

/// Assemble the final output string from globals + sorted functions.
fn assemble(
    globals: &[String],
    sorted_funcs: &[String],
    functions: &HashMap<String, String>,
) -> String {
    let mut out = String::new();

    if !globals.is_empty() {
        out.push_str("globals\n");
        for g in globals {
            out.push_str("    ");
            out.push_str(g.trim());
            out.push('\n');
        }
        out.push_str("endglobals\n\n");
    }

    for fname in sorted_funcs {
        if let Some(fsrc) = functions.get(fname) {
            out.push_str(fsrc.trim());
            out.push_str("\n\n");
        }
    }

    out
}

#[derive(Debug, Clone)]
struct FrozenImportEntry {
    is_ujapi: bool,
    url: Url,
}

/// Emit frozen import directives at the top of the generated script.
fn emit_frozen_import_header(entries: &[FrozenImportEntry], out_dir: &Path) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut header = String::new();
    for entry in entries {
        let frozen_path = match entry.url.to_file_path() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let rel = relative_path(out_dir, &frozen_path);
        let directive = if entry.is_ujapi { "//import-ujapi!" } else { "//import!" };
        header.push_str(directive);
        header.push(' ');
        header.push_str(&rel.replace('\\', "/"));
        header.push('\n');
    }
    header.push('\n');
    header
}

/// Compute a relative path from `from_dir` to `to_file`.
fn relative_path(from_dir: &Path, to_file: &Path) -> String {
    let from = from_dir.canonicalize().unwrap_or_else(|_| from_dir.to_path_buf());
    let to = to_file.canonicalize().unwrap_or_else(|_| to_file.to_path_buf());

    let from_parts: Vec<_> = from.components().collect();
    let to_parts: Vec<_> = to.components().collect();

    let common = from_parts
        .iter()
        .zip(to_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut result = String::new();
    for _ in common..from_parts.len() {
        if !result.is_empty() {
            result.push('/');
        }
        result.push_str("..");
    }
    for part in &to_parts[common..] {
        if !result.is_empty() {
            result.push('/');
        }
        result.push_str(&part.as_os_str().to_string_lossy());
    }

    if result.is_empty() {
        to_file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        result
    }
}

/// Apply stable output formatting for generated scripts.
fn normalize_output(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 16);
    let mut blank_run = 0usize;

    for line in raw.lines() {
        let trimmed_end = line.trim_end();
        if trimmed_end.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
            out.push('\n');
            continue;
        }
        blank_run = 0;
        out.push_str(trimmed_end);
        out.push('\n');
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}


