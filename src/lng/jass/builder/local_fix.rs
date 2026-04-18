//! Local single-file fixer for diagnostics that don't require cross-file context.
//!
//! Current scope:
//! - handle leak fixes (`code == "leak"`)
//!
//! The fixer can run in preview mode (`write_output = false`) or write changes
//! back to the source file.

use crate::http::diagnostic::Diagnostic;
use crate::lng::jass::ast::{build_ast, rewrite_imports, Ast, FunctionDecl, Statement, VarStmt};
use crate::lng::jass::builder::BuildResult;
use crate::lng::jass::cursor::Cursor;
use crate::util::roper::uri_map::ROPE_MAP;
use lapce_xi_rope::Rope;
use std::collections::{HashMap, HashSet};
use url::Url;

use super::collect::{has_build_opt, read_source};

/// Run local fixes for one file.
///
/// When `write_output` is false, runs analysis and patch construction but does
/// not persist the modified text.
pub fn fix_local(uri: &Url, write_output: bool) -> BuildResult {
    let src = match read_source(uri) {
        Some(s) => s,
        None => return BuildResult::err("cannot read source file"),
    };

    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_jass::language().into())
        .is_err()
    {
        return BuildResult::err("cannot set jass parser language");
    }
    let tree = match parser.parse(&src, None) {
        Some(t) => t,
        None => return BuildResult::err("cannot parse jass file"),
    };

    let mut ast = build_ast(tree.root_node());
    rewrite_imports(&mut ast, src.as_bytes());

    // One AST pass builds all structural anchors for local fixes.
    let index = build_ast_fix_index(&ast, &src);

    let rope = Rope::from(src.as_str());
    let cursor = Cursor::walk(&ast, &rope, &[]);
    let leak_fix_method = if has_build_opt(&cursor.file_settings, "nolocal") {
        LeakFixMethod::GlobalTemp
    } else {
        LeakFixMethod::LocalTemp
    };

    let leak_diags: Vec<Diagnostic> = cursor
        .diagnostics
        .into_iter()
        .filter(|d| d.has_code("leak"))
        .filter(|d| d.data.is_some())
        .collect();

    if leak_diags.is_empty() {
        return BuildResult::ok(
            String::new(),
            "local-fix: no applicable diagnostics".to_string(),
        );
    }

    let edits = collect_leak_edits(&leak_diags, &index, &src, leak_fix_method);
    if edits.is_empty() {
        return BuildResult::ok(
            String::new(),
            "local-fix: no edits produced".to_string(),
        );
    }

    let fixed = apply_line_edits(&src, &edits);

    if write_output {
        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return BuildResult::err("uri is not a file path"),
        };

        if let Err(e) = std::fs::write(&path, &fixed) {
            return BuildResult::err(&format!("failed to write fixed file: {e}"));
        }

        // Keep open-buffer mirror in sync for immediate downstream operations.
        ROPE_MAP.insert(uri.clone(), Rope::from(fixed.as_str()));

        BuildResult::ok(
            path.display().to_string(),
            format!("local-fix: applied {} edit(s)", edits.len()),
        )
    } else {
        BuildResult::ok(
            String::new(),
            format!("local-fix preview: {} edit(s)", edits.len()),
        )
    }
}

#[derive(Debug, Clone)]
struct LineEdit {
    start_line: usize,
    end_line: usize,
    new_text: String,
}

#[derive(Debug, Clone)]
struct FunctionFixIndex {
    endfunction_line: usize,
    body_indent: String,
    local_insert_line: usize,
    return_lines: HashSet<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeakFixMethod {
    /// Use a local temp variable for returned-local rewrites.
    LocalTemp,
    /// Do not introduce a local temp; use a global temp variable instead.
    GlobalTemp,
}

#[derive(Debug, Clone)]
struct AstFixIndex {
    endglobals_line: Option<usize>,
    line_indents: Vec<String>,
    functions: HashMap<String, FunctionFixIndex>,
    /// Every identifier name declared anywhere in the file (globals, params,
    /// locals).  Used for collision-free synthetic name generation.
    declared_names: HashSet<String>,
}

impl AstFixIndex {
    fn line_indent(&self, line: usize) -> String {
        self.line_indents.get(line).cloned().unwrap_or_default()
    }

    fn body_indent_for_diag(&self, diag: &Diagnostic, target_line: usize) -> String {
        if let Some(func_name) = leak_func_name(diag) {
            if let Some(fi) = self.functions.get(&func_name) {
                return fi.body_indent.clone();
            }
        }

        if let Some(fi) = self
            .functions
            .values()
            .find(|fi| fi.endfunction_line == target_line)
        {
            return fi.body_indent.clone();
        }

        self.line_indent(target_line)
    }

    fn is_known_return_line(&self, diag: &Diagnostic, line: usize) -> bool {
        if let Some(func_name) = leak_func_name(diag) {
            if let Some(fi) = self.functions.get(&func_name) {
                return fi.return_lines.contains(&line);
            }
        }
        true
    }
}

fn build_ast_fix_index(ast: &Ast, src: &str) -> AstFixIndex {
    let lines: Vec<&str> = src.split('\n').collect();
    let mut line_indents = Vec::with_capacity(lines.len());
    for line in &lines {
        line_indents.push(
            line.chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect(),
        );
    }

    let mut endglobals_line = None;
    let mut functions = HashMap::<String, FunctionFixIndex>::new();

    for item in &ast.items {
        match item {
            Statement::Globals(g) => {
                if endglobals_line.is_none() {
                    endglobals_line = find_keyword_line_in_range(
                        &lines,
                        g.node.start_position().row,
                        g.node.end_position().row,
                        "endglobals",
                    );
                }
            }
            Statement::Function(f) => {
                let name = match f.name.as_ref() {
                    Some(id) => &src[id.node.start_byte()..id.node.end_byte()],
                    None => continue,
                };
                if name.is_empty() || functions.contains_key(name) {
                    continue;
                }

                let end_line = find_keyword_line_in_range(
                    &lines,
                    f.node.start_position().row,
                    f.node.end_position().row,
                    "endfunction",
                )
                .unwrap_or(f.node.end_position().row.min(lines.len().saturating_sub(1)));

                let body_indent = find_function_body_indent(&lines, f, end_line, &line_indents);
                let local_insert_line = find_local_insert_line(f, end_line);

                let mut return_lines = HashSet::new();
                collect_return_lines(&f.body, &mut return_lines);

                functions.insert(
                    name.to_string(),
                    FunctionFixIndex {
                        endfunction_line: end_line,
                        body_indent,
                        local_insert_line,
                        return_lines,
                    },
                );
            }
            _ => {}
        }
    }

    AstFixIndex {
        endglobals_line,
        line_indents,
        functions,
        declared_names: collect_all_declared_names(ast, src),
    }
}

/// Collect every declared identifier name from the AST (globals, params,
/// locals inside all functions).
fn collect_all_declared_names(ast: &Ast, src: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &ast.items {
        match item {
            Statement::Globals(g) => collect_var_stmt_names_into(src, &g.vars, &mut names),
            Statement::VarStmt(v) => collect_single_var_names_into(src, v, &mut names),
            Statement::Function(f) => {
                for p in &f.params {
                    if let Some(id) = &p.name {
                        names.insert(src[id.node.start_byte()..id.node.end_byte()].to_string());
                    }
                }
                collect_stmts_declared_names(src, &f.body, &mut names);
            }
            _ => {}
        }
    }
    names
}

fn collect_var_stmt_names_into(src: &str, vars: &[VarStmt<'_>], out: &mut HashSet<String>) {
    for v in vars {
        collect_single_var_names_into(src, v, out);
    }
}

fn collect_single_var_names_into(src: &str, v: &VarStmt<'_>, out: &mut HashSet<String>) {
    for d in &v.decls {
        if let Some(id) = &d.name {
            out.insert(src[id.node.start_byte()..id.node.end_byte()].to_string());
        }
    }
}

fn collect_stmts_declared_names(src: &str, stmts: &[Statement<'_>], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Statement::Local(l) => {
                if let Some(id) = &l.name {
                    out.insert(src[id.node.start_byte()..id.node.end_byte()].to_string());
                }
            }
            Statement::VarStmt(v) => collect_single_var_names_into(src, v, out),
            Statement::If(s) => {
                collect_stmts_declared_names(src, &s.body, out);
                for b in &s.branches {
                    collect_stmts_declared_names(src, &b.body, out);
                }
            }
            Statement::Loop(s) => collect_stmts_declared_names(src, &s.body, out),
            _ => {}
        }
    }
}

fn find_local_insert_line(f: &FunctionDecl, end_line: usize) -> usize {
    f.body
        .first()
        .map(|stmt| match stmt {
            Statement::Type(s) => s.node.start_position().row,
            Statement::Native(s) => s.node.start_position().row,
            Statement::Function(s) => s.node.start_position().row,
            Statement::Globals(s) => s.node.start_position().row,
            Statement::Local(s) => s.node.start_position().row,
            Statement::Set(s) => s.node.start_position().row,
            Statement::Call(s) => s.node.start_position().row,
            Statement::Return(s) => s.node.start_position().row,
            Statement::Exitwhen(s) => s.node.start_position().row,
            Statement::If(s) => s.node.start_position().row,
            Statement::Loop(s) => s.node.start_position().row,
            Statement::VarStmt(s) => s.node.start_position().row,
            Statement::Comment(s) => s.node.start_position().row,
            Statement::Import(s) => s.node.start_position().row,
            Statement::SetDir(s) => s.node.start_position().row,
            Statement::IgnoreDir(s) => s.node.start_position().row,
            Statement::UjapiImport(s) => s.node.start_position().row,
            Statement::EntryDir(s) => s.node.start_position().row,
            Statement::Error(s) => s.node.start_position().row,
        })
        .unwrap_or(end_line)
}

fn find_keyword_line_in_range(
    lines: &[&str],
    start_line: usize,
    end_line: usize,
    keyword: &str,
) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }
    let last = lines.len() - 1;
    let start = start_line.min(last);
    let end = end_line.min(last);
    for line in start..=end {
        if lines[line].trim() == keyword {
            return Some(line);
        }
    }
    None
}

fn find_function_body_indent(
    lines: &[&str],
    f: &FunctionDecl,
    end_line: usize,
    line_indents: &[String],
) -> String {
    for stmt in &f.body {
        let line = match stmt {
            Statement::Type(s) => s.node.start_position().row,
            Statement::Native(s) => s.node.start_position().row,
            Statement::Function(s) => s.node.start_position().row,
            Statement::Globals(s) => s.node.start_position().row,
            Statement::Local(s) => s.node.start_position().row,
            Statement::Set(s) => s.node.start_position().row,
            Statement::Call(s) => s.node.start_position().row,
            Statement::Return(s) => s.node.start_position().row,
            Statement::Exitwhen(s) => s.node.start_position().row,
            Statement::If(s) => s.node.start_position().row,
            Statement::Loop(s) => s.node.start_position().row,
            Statement::VarStmt(s) => s.node.start_position().row,
            Statement::Comment(s) => s.node.start_position().row,
            Statement::Import(s) => s.node.start_position().row,
            Statement::SetDir(s) => s.node.start_position().row,
            Statement::IgnoreDir(s) => s.node.start_position().row,
            Statement::UjapiImport(s) => s.node.start_position().row,
            Statement::EntryDir(s) => s.node.start_position().row,
            Statement::Error(s) => s.node.start_position().row,
        };
        if line < lines.len() && !lines[line].trim().is_empty() {
            return line_indents.get(line).cloned().unwrap_or_default();
        }
    }
    format!(
        "{}    ",
        line_indents
            .get(end_line)
            .cloned()
            .unwrap_or_default()
    )
}

fn collect_return_lines(stmts: &[Statement], out: &mut HashSet<usize>) {
    for stmt in stmts {
        match stmt {
            Statement::Return(r) => {
                out.insert(r.node.start_position().row);
            }
            Statement::If(i) => {
                collect_return_lines(&i.body, out);
                for b in &i.branches {
                    collect_return_lines(&b.body, out);
                }
            }
            Statement::Loop(l) => collect_return_lines(&l.body, out),
            _ => {}
        }
    }
}


fn collect_leak_edits(
    diags: &[Diagnostic],
    index: &AstFixIndex,
    src: &str,
    method: LeakFixMethod,
) -> Vec<LineEdit> {
    let mut edits = Vec::new();
    let mut seen = HashSet::new();
    // Track names we've added in this pass to avoid collisions between fixes
    let mut generated_names = index.declared_names.clone();

    for diag in diags {
        let var = match leak_var(diag) {
            Some(v) => v,
            None => continue,
        };
        let key = (diag.range.start.line, var.clone());
        if !seen.insert(key) {
            continue;
        }

        if is_returned_local(diag) {
            let fix_edits = returned_local_edits_with_tracking(
                diag,
                index,
                method,
                &mut generated_names,
            );
            edits.extend(fix_edits);
        } else if let Some(edit) = leak_text_edit(diag, index) {
            edits.push(edit);
        }
    }

    edits
}

fn leak_var(diag: &Diagnostic) -> Option<String> {
    diag.data.as_ref()?.get("leak_var")?.as_str().map(String::from)
}

fn leak_kind(diag: &Diagnostic) -> Option<String> {
    diag.data.as_ref()?.get("leak_kind")?.as_str().map(String::from)
}

fn leak_type(diag: &Diagnostic) -> Option<String> {
    diag.data.as_ref()?.get("leak_type")?.as_str().map(String::from)
}

fn leak_func_name(diag: &Diagnostic) -> Option<String> {
    diag.data.as_ref()?.get("func_name")?.as_str().map(String::from)
}

fn is_returned_local(diag: &Diagnostic) -> bool {
    diag.data
        .as_ref()
        .and_then(|d| d.get("returned_local"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn unique_global_name(func_name: &str, var_name: &str, declared: &HashSet<String>) -> String {
    let base = format!("{}_{}", func_name, var_name);
    if !declared.contains(&base) {
        return base;
    }
    let mut suffix = 2u32;
    loop {
        let candidate = format!("{}_{}", base, suffix);
        if !declared.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

fn unique_local_name(func_name: &str, var_name: &str, declared: &HashSet<String>) -> String {
    let base = format!("{}_{}_ret", func_name, var_name);
    if !declared.contains(&base) {
        return base;
    }
    let mut suffix = 2u32;
    loop {
        let candidate = format!("{}_{}", base, suffix);
        if !declared.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}


fn returned_local_edits(
    diag: &Diagnostic,
    index: &AstFixIndex,
    method: LeakFixMethod,
) -> Vec<LineEdit> {
    let mut generated = index.declared_names.clone();
    returned_local_edits_with_tracking(diag, index, method, &mut generated)
}

fn returned_local_edits_with_tracking(
    diag: &Diagnostic,
    index: &AstFixIndex,
    method: LeakFixMethod,
    generated_names: &mut HashSet<String>,
) -> Vec<LineEdit> {
    let var = match leak_var(diag) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let type_name = match leak_type(diag) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let func_name = match leak_func_name(diag) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let mut edits = Vec::new();
    let ret_line = diag.range.start.line;
    if !index.is_known_return_line(diag, ret_line) {
        return edits;
    }

    let indent = index.line_indent(ret_line);

    match method {
        LeakFixMethod::GlobalTemp => {
            let global_name = unique_global_name(&func_name, &var, generated_names);
            // Track that we've used this name
            generated_names.insert(global_name.clone());

            if let Some(endglobals_line) = index.endglobals_line {
                let glob_indent = index.line_indent(endglobals_line);
                edits.push(LineEdit {
                    start_line: endglobals_line,
                    end_line: endglobals_line,
                    new_text: format!("{}{} {}\n", glob_indent, type_name, global_name),
                });
            } else {
                edits.push(LineEdit {
                    start_line: 0,
                    end_line: 0,
                    new_text: format!("globals\n    {} {}\nendglobals\n\n", type_name, global_name),
                });
            }

            edits.push(LineEdit {
                start_line: ret_line,
                end_line: ret_line + 1,
                new_text: format!(
                    "{indent}set {global} = {var}\n{indent}set {var} = null\n{indent}return {global}\n",
                    indent = indent,
                    global = global_name,
                    var = var,
                ),
            });
        }
        LeakFixMethod::LocalTemp => {
            let local_name = unique_local_name(&func_name, &var, generated_names);
            // Track that we've used this name
            generated_names.insert(local_name.clone());

            let (local_insert_line, local_indent) = index
                .functions
                .get(&func_name)
                .map(|fi| (fi.local_insert_line, fi.body_indent.clone()))
                .unwrap_or((ret_line, indent.clone()));

            edits.push(LineEdit {
                start_line: local_insert_line,
                end_line: local_insert_line,
                new_text: format!("{}local {} {}\n", local_indent, type_name, local_name),
            });

            edits.push(LineEdit {
                start_line: ret_line,
                end_line: ret_line + 1,
                new_text: format!(
                    "{indent}set {local} = {var}\n{indent}set {var} = null\n{indent}return {local}\n",
                    indent = indent,
                    local = local_name,
                    var = var,
                ),
            });
        }
    }

    edits
}

fn leak_text_edit(diag: &Diagnostic, index: &AstFixIndex) -> Option<LineEdit> {
    let var = leak_var(diag)?;
    let kind = leak_kind(diag)?;
    let target_line = diag.range.start.line;

    let indent = if kind == "endfunction" {
        index.body_indent_for_diag(diag, target_line)
    } else {
        index.line_indent(target_line)
    };

    Some(LineEdit {
        start_line: target_line,
        end_line: target_line,
        new_text: format!("{}set {} = null\n", indent, var),
    })
}

fn line_offset(text: &str, line: usize) -> usize {
    if line == 0 {
        return 0;
    }
    let mut cur_line = 0usize;
    for (idx, ch) in text.char_indices() {
        if ch == '\n' {
            cur_line += 1;
            if cur_line == line {
                return idx + 1;
            }
        }
    }
    text.len()
}

fn apply_line_edits(text: &str, edits: &[LineEdit]) -> String {
    let mut out = text.to_string();
    let mut sorted = edits.to_vec();

    // Apply from bottom to top so earlier offsets stay stable.
    sorted.sort_by(|a, b| {
        b.start_line
            .cmp(&a.start_line)
            .then_with(|| b.end_line.cmp(&a.end_line))
    });

    for e in &sorted {
        let start = line_offset(&out, e.start_line);
        let end = line_offset(&out, e.end_line);
        if start <= end && end <= out.len() {
            out.replace_range(start..end, &e.new_text);
        }
    }

    out
}

#[cfg(test)]
#[path = "local_fix_test.rs"]
mod local_fix_test;
