use crate::lsp::code_action::lsp::{
    CodeAction, CodeActionParams, Command, CODE_ACTION_KIND_QUICKFIX, CODE_ACTION_KIND_REFACTOR,
};
use crate::lsp::diagnostic::lsp::Diagnostic;
use crate::lsp::position::Position;
use crate::lsp::range::Range;
use crate::http::rename::{TextEdit, WorkspaceEdit};
use crate::util::file_store::FILE_STORE;
use crate::util::open::is_as_uri;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::tree_map::TREE_MAP;
use serde_json::json;
use std::collections::HashMap;
use tree_sitter::Point;


pub(crate) fn compute(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    // ── UjAPI download / re-download actions ──────────────────────────────
    // Diagnostics with source="ujapi" carry { ujapi_uri, ujapi_path } in `data`.
    let ujapi_diags: Vec<_> = params.context.diagnostics.iter()
        .filter(|d| d.has_code("ujapi"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    if !ujapi_diags.is_empty() {
        // Extract download params from the first matching diagnostic.
        let maybe_params = ujapi_diags[0].data.as_ref().and_then(|data| {
            let u = data.get("ujapi_uri")?.as_str()?.to_string();
            let p = data.get("ujapi_path")?.as_str()?.to_string();
            if u.is_empty() || p.is_empty() { None } else { Some((u, p)) }
        });

        if let Some((ujapi_uri, ujapi_path)) = maybe_params {
            let is_not_found = ujapi_diags.iter().any(|d| d.message.contains("not found"));
            let title = if is_not_found {
                crate::util::i18n::ujapi_download()
            } else {
                crate::util::i18n::ujapi_update()
            };

            actions.push(CodeAction {
                title: title.to_string(),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(ujapi_diags),
                edit: None,
                command: Some(Command {
                    title: title.to_string(),
                    command: "ujapi.download".into(),
                    arguments: Some(vec![
                        json!(ujapi_uri),
                        json!(ujapi_path),
                    ]),
                }),
            });
        }
    }

    // ── AS string format toggle: "…" ↔ """…""" ───────────────────────────
    let uri = &params.text_document.uri;
    if is_as_uri(uri) {
        if let Some((single_action, file_action)) = compute_as_string_toggle(params) {
            actions.push(single_action);
            if let Some(fa) = file_action {
                actions.push(fa);
            }
        }
    }

    // ── Handle leak quick fixes ─────────────────────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_leak_fixes(params));
    }

    // ── Unused function quick fixes ───────────────────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_unused_func_fixes(params));
    }

    // ── Simplify if-return quick fixes ──────────────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_simplify_fixes(params));
    }

    // ── Redundant parentheses quick fixes ───────────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_parens_fixes(params));
    }

    // ── Redundant boolean-comparison quick fixes ─────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_bool_cmp_fixes(params));
    }

    // ── Inline single-call function quick fixes ──────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_inline_fixes(params));
    }

    // ── Collapse and-chain quick fixes ────────────────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_collapse_and_fixes(params));
    }

    // ── Collapse or-chain quick fixes ─────────────────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_collapse_or_fixes(params));
    }

    // ── Empty else quick fixes ──────────────────────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_empty_else_fixes(params));
    }

    // ── Remove else branch refactoring ──────────────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        if let Some(action) = compute_remove_else_action(params) {
            actions.push(action);
        }
    }

    // ── Fold StringHash refactoring ──────────────────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_string_hash_fold(params));
    }

    // ── ExecuteFunc quick fixes ──────────────────────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_execute_func_fixes(params));
    }

    // ── `else if` → `elseif` quick fixes ─────────────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_else_if_fixes(params));
    }

    // ── Array initializer quick fixes ──────────────────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_array_no_init_fixes(params));
    }

    // ── Array set without index quick fixes ────────────────────────────────
    let uri = &params.text_document.uri;
    if !is_as_uri(uri) {
        actions.extend(compute_array_set_no_index_fixes(params));
    }

    actions
}

// ─── Simplify if-return fixes ────────────────────────────────────────────────

/// Quick-fix and "fix all" actions for redundant if-return patterns.
fn compute_simplify_fixes(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    let uri = &params.text_document.uri;
    let rope = match ROPE_MAP.get(uri) {
        Some(r) => r,
        None => return actions,
    };
    let _rope = rope.value();

    // Per-diagnostic quick fixes from the current request context.
    let simplify_diags: Vec<_> = params
        .context
        .diagnostics
        .iter()
        .filter(|d| d.has_code("simplify"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    for diag in &simplify_diags {
        if let Some(new_text) = diag
            .data
            .as_ref()
            .and_then(|d| d.get("simplify_new_text"))
            .and_then(|v| v.as_str())
        {
            let edit = TextEdit {
                range: diag.range.clone(),
                new_text: new_text.to_string(),
            };
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![edit]);
            actions.push(CodeAction {
                title: crate::util::i18n::simplify_if_return_action().to_string(),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit { changes: Some(changes) }),
                command: None,
            });
        }
    }

    // "Fix all" action (needs ≥ 2 simplifiable patterns in the file).
    if !simplify_diags.is_empty() {
        if let Some(file_action) = compute_simplify_fix_all(uri) {
            actions.push(file_action);
        }
    }

    actions
}

/// Build a single code action that fixes ALL redundant if-returns in the file.
fn compute_simplify_fix_all(uri: &url::Url) -> Option<CodeAction> {
    let snap = FILE_STORE.get(uri)?;
    let all_diags = &snap.value().diagnostics;

    let simplify_diags: Vec<_> = all_diags
        .iter()
        .filter(|d| d.has_code("simplify"))
        .filter(|d| d.data.is_some())
        .collect();

    if simplify_diags.len() < 2 {
        return None;
    }

    // Sort ascending by (line, character).
    let mut edits: Vec<(usize, usize, TextEdit)> = Vec::new();
    for diag in &simplify_diags {
        if let Some(new_text) = diag
            .data
            .as_ref()
            .and_then(|d| d.get("simplify_new_text"))
            .and_then(|v| v.as_str())
        {
            edits.push((
                diag.range.start.line,
                diag.range.start.character,
                TextEdit {
                    range: diag.range.clone(),
                    new_text: new_text.to_string(),
                },
            ));
        }
    }

    edits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let text_edits: Vec<TextEdit> = edits.into_iter().map(|(_, _, e)| e).collect();

    if text_edits.is_empty() {
        return None;
    }

    let title = crate::util::i18n::simplify_all_if_return_action().to_string();
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), text_edits);

    Some(CodeAction {
        title,
        kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
        diagnostics: None,
        edit: Some(WorkspaceEdit { changes: Some(changes) }),
        command: None,
    })
}

// ─── Redundant parentheses fixes ─────────────────────────────────────────────

/// Extract the two 1-character delete edits stored in a `parens` diagnostic.
///
/// Each diagnostic stores the position of its `(` (`parens_open`) and `)`
/// (`parens_close`) as 1-char LSP ranges.  Deleting those two characters
/// is equivalent to removing the paren pair but guarantees that edits for
/// different diagnostics — even when the paren spans overlap — are always
/// non-overlapping.
fn paren_delete_edits(diag: &Diagnostic) -> Option<[TextEdit; 2]> {
    let data = diag.data.as_ref()?;
    let open: Range  = serde_json::from_value(data.get("parens_open")?.clone()).ok()?;
    let close: Range = serde_json::from_value(data.get("parens_close")?.clone()).ok()?;
    Some([
        TextEdit { range: open,  new_text: String::new() },
        TextEdit { range: close, new_text: String::new() },
    ])
}

/// Quick-fix and "fix all" actions for redundant parentheses diagnostics.
fn compute_parens_fixes(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    let uri = &params.text_document.uri;

    let parens_diags: Vec<_> = params
        .context
        .diagnostics
        .iter()
        .filter(|d| d.has_code("parens"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    for diag in &parens_diags {
        if let Some([open_edit, close_edit]) = paren_delete_edits(diag) {
            let mut changes = HashMap::new();
            // Close first so that if a client applies sequentially the open
            // position is still valid (deleting `)` doesn't shift `(`).
            changes.insert(uri.clone(), vec![close_edit, open_edit]);
            actions.push(CodeAction {
                title: crate::util::i18n::remove_redundant_parens().to_string(),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit { changes: Some(changes) }),
                command: None,
            });
        }
    }

    if !parens_diags.is_empty() {
        if let Some(file_action) = compute_parens_fix_all(uri) {
            actions.push(file_action);
        }
    }

    actions
}

/// Build a single code action that removes ALL redundant parentheses in the file.
///
/// Each diagnostic contributes two 1-char delete edits (`(` and `)`).
/// Because every edit covers exactly one character, no two edits can ever
/// overlap — even for nested paren pairs like `(Some((1)))`.
///
/// Edits are sorted descending by (line, character) so that, even if the
/// client applies them sequentially, later edits in the file don't shift the
/// positions of earlier ones.
fn compute_parens_fix_all(uri: &url::Url) -> Option<CodeAction> {
    let snap = FILE_STORE.get(uri)?;
    let all_diags = &snap.value().diagnostics;

    let parens_diags: Vec<_> = all_diags
        .iter()
        .filter(|d| d.has_code("parens"))
        .filter(|d| d.data.is_some())
        .collect();

    if parens_diags.len() < 2 {
        return None;
    }

    // Collect (line, char, TextEdit) for every open and close paren.
    let mut edits: Vec<(usize, usize, TextEdit)> = Vec::new();
    for diag in &parens_diags {
        if let Some([open_edit, close_edit]) = paren_delete_edits(diag) {
            edits.push((open_edit.range.start.line,  open_edit.range.start.character,  open_edit));
            edits.push((close_edit.range.start.line, close_edit.range.start.character, close_edit));
        }
    }

    if edits.is_empty() {
        return None;
    }

    // Sort descending by (line, character): apply end-of-file first so that
    // earlier positions are not shifted by preceding deletions.
    edits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let text_edits: Vec<TextEdit> = edits.into_iter().map(|(_, _, e)| e).collect();

    let title = crate::util::i18n::remove_all_redundant_parens().to_string();
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), text_edits);

    Some(CodeAction {
        title,
        kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
        diagnostics: None,
        edit: Some(WorkspaceEdit { changes: Some(changes) }),
        command: None,
    })
}



// ─── Redundant boolean comparison fixes ──────────────────────────────────────

/// Quick-fix and "fix all" actions for `expr == true/false` / `expr != true/false`.
fn compute_bool_cmp_fixes(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();
    let uri = &params.text_document.uri;

    let diags: Vec<_> = params
        .context
        .diagnostics
        .iter()
        .filter(|d| d.has_code("bool-cmp"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    for diag in &diags {
        if let Some(new_text) = diag
            .data.as_ref()
            .and_then(|d| d.get("bool_cmp_new_text"))
            .and_then(|v| v.as_str())
        {
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![TextEdit {
                range: diag.range.clone(),
                new_text: new_text.to_string(),
            }]);
            actions.push(CodeAction {
                title: crate::util::i18n::simplify_bool_cmp().to_string(),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit { changes: Some(changes) }),
                command: None,
            });
        }
    }

    if !diags.is_empty() {
        if let Some(file_action) = compute_bool_cmp_fix_all(uri) {
            actions.push(file_action);
        }
    }

    actions
}

/// Build a code action that simplifies ALL redundant boolean comparisons in the file.
fn compute_bool_cmp_fix_all(uri: &url::Url) -> Option<CodeAction> {
    let snap = FILE_STORE.get(uri)?;
    let all_diags = &snap.value().diagnostics;

    let bool_diags: Vec<_> = all_diags
        .iter()
        .filter(|d| d.has_code("bool-cmp"))
        .filter(|d| d.data.is_some())
        .collect();

    if bool_diags.len() < 2 {
        return None;
    }

    let mut edits: Vec<(usize, usize, TextEdit)> = Vec::new();
    for diag in &bool_diags {
        if let Some(new_text) = diag
            .data.as_ref()
            .and_then(|d| d.get("bool_cmp_new_text"))
            .and_then(|v| v.as_str())
        {
            edits.push((
                diag.range.start.line,
                diag.range.start.character,
                TextEdit { range: diag.range.clone(), new_text: new_text.to_string() },
            ));
        }
    }

    if edits.is_empty() {
        return None;
    }

    // Sort descending: apply end-of-file edits first so earlier positions stay valid.
    edits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let text_edits: Vec<TextEdit> = edits.into_iter().map(|(_, _, e)| e).collect();

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), text_edits);

    Some(CodeAction {
        title: crate::util::i18n::simplify_all_bool_cmp().to_string(),
        kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
        diagnostics: None,
        edit: Some(WorkspaceEdit { changes: Some(changes) }),
        command: None,
    })
}

const STRING_LITERAL_KIND: u16 = crate::lng::ass::kind::Kind::StringLiteral as u16;

/// If the cursor sits inside a `string_literal` node, propose converting
/// between `"…"` and `"""…"""`.  Handles escaping:
/// - `"…"` → `"""…"""`: remove `\"` escapes (turn into plain `"`)
/// - `"""…"""` → `"…"`: add `\"` escapes for every bare `"` inside
///
/// Returns `(single_action, Option<file_wide_action>)`.
fn compute_as_string_toggle(params: &CodeActionParams) -> Option<(CodeAction, Option<CodeAction>)> {
    let uri = &params.text_document.uri;

    let rope = ROPE_MAP.get(uri)?;
    let rope = rope.value();
    let tree = TREE_MAP.get(uri)?;
    let tree = tree.value();

    let point = Point {
        row: params.range.start.line,
        column: params.range.start.character,
    };

    let root = tree.root_node();
    let node = root.descendant_for_point_range(point, point)?;

    // Walk up to find the nearest string_literal node.
    let string_node = {
        let mut n = node;
        loop {
            if n.grammar_id() == STRING_LITERAL_KIND {
                break Some(n);
            }
            n = n.parent()?;
        }
    }?;

    let sb = string_node.start_byte();
    let eb = string_node.end_byte();
    let text = rope.slice_to_cow(sb..eb);
    let text = text.as_ref();

    // Determine current format.
    let is_triple = text.starts_with("\"\"\"");

    // ── Single-string action ─────────────────────────────────────────────
    let (title, new_text) = convert_string_text(text, is_triple);

    let range = Range::from_byte_offsets(rope, sb, eb);

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![TextEdit { range, new_text }]);

    let single_action = CodeAction {
        title: title.to_string(),
        kind: Some(CODE_ACTION_KIND_REFACTOR.into()),
        diagnostics: None,
        edit: Some(WorkspaceEdit { changes: Some(changes) }),
        command: None,
    };

    // ── File-wide action ─────────────────────────────────────────────────
    // Collect every string_literal in the file that has the same format as
    // the one under the cursor, and convert them all at once.
    let file_action = compute_file_wide_toggle(uri, &root, rope, is_triple);

    Some((single_action, file_action))
}

/// Walk the whole CST and convert every `string_literal` that matches the
/// source format (`is_triple`) to the opposite format.
fn compute_file_wide_toggle(
    uri: &url::Url,
    root: &tree_sitter::Node,
    rope: &lapce_xi_rope::Rope,
    is_triple: bool,
) -> Option<CodeAction> {
    let mut edits: Vec<TextEdit> = Vec::new();

    let mut cursor = root.walk();
    collect_string_edits(&mut cursor, rope, is_triple, &mut edits);

    // Only offer the file-wide action when there are ≥ 2 matching strings.
    if edits.len() < 2 {
        return None;
    }

    let title = if is_triple {
        crate::util::i18n::convert_all_to_single_quoted()
    } else {
        crate::util::i18n::convert_all_to_triple_quoted()
    };

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);

    Some(CodeAction {
        title: title.to_string(),
        kind: Some(CODE_ACTION_KIND_REFACTOR.into()),
        diagnostics: None,
        edit: Some(WorkspaceEdit { changes: Some(changes) }),
        command: None,
    })
}

/// Recursively walk the tree and collect `TextEdit`s for every `string_literal`
/// whose format matches `is_triple`.
fn collect_string_edits(
    cursor: &mut tree_sitter::TreeCursor,
    rope: &lapce_xi_rope::Rope,
    is_triple: bool,
    edits: &mut Vec<TextEdit>,
) {
    loop {
        let node = cursor.node();

        if node.grammar_id() == STRING_LITERAL_KIND {
            let sb = node.start_byte();
            let eb = node.end_byte();
            let text = rope.slice_to_cow(sb..eb);
            let text = text.as_ref();

            let node_is_triple = text.starts_with("\"\"\"");
            if node_is_triple == is_triple {
                let (_title, new_text) = convert_string_text(text, is_triple);
                let range = Range::from_byte_offsets(rope, sb, eb);
                edits.push(TextEdit { range, new_text });
            }
        } else if cursor.goto_first_child() {
            collect_string_edits(cursor, rope, is_triple, edits);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Convert one string literal text to the opposite format.
/// Returns `(localized_title, new_text)`.
fn convert_string_text(text: &str, is_triple: bool) -> (&'static str, String) {
    if is_triple {
        let inner = strip_triple_quotes(text);
        let escaped = escape_for_single(inner);
        (
            crate::util::i18n::convert_to_single_quoted(),
            format!("\"{}\"", escaped),
        )
    } else {
        let inner = strip_single_quotes(text);
        let unescaped = unescape_for_triple(inner);
        (
            crate::util::i18n::convert_to_triple_quoted(),
            format!("\"\"\"{}\"\"\"", unescaped),
        )
    }
}

/// Strip surrounding `"""` from a triple-quoted string, returning the inner content.
fn strip_triple_quotes(s: &str) -> &str {
    let s = if s.starts_with("\"\"\"") { &s[3..] } else { s };
    if s.ends_with("\"\"\"") { &s[..s.len() - 3] } else { s }
}

/// Strip surrounding `"` from a single-quoted string, returning the inner content.
fn strip_single_quotes(s: &str) -> &str {
    let s = if s.starts_with('"') { &s[1..] } else { s };
    if s.ends_with('"') { &s[..s.len() - 1] } else { s }
}

/// Escape a triple-quoted inner string for use inside single quotes.
/// Every bare `"` must become `\"`. Existing backslash sequences are kept as-is.
fn escape_for_single(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            // Keep existing escape sequence as-is.
            out.push(bytes[i] as char);
            out.push(bytes[i + 1] as char);
            i += 2;
        } else if bytes[i] == b'"' {
            out.push_str("\\\"");
            i += 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Unescape a single-quoted inner string for use inside triple quotes.
/// `\"` becomes plain `"`. Other backslash sequences are kept.
fn unescape_for_triple(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'"' {
                // \" → "
                out.push('"');
                i += 2;
            } else {
                // Keep other escapes as-is.
                out.push(bytes[i] as char);
                out.push(bytes[i + 1] as char);
                i += 2;
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

// ─── Handle leak quick fixes ─────────────────────────────────────────────────

/// Extract variable name from a leak diagnostic's `data` field.
fn leak_var(diag: &Diagnostic) -> Option<String> {
    diag.data.as_ref()?.get("leak_var")?.as_str().map(String::from)
}

/// Extract leak kind (`"return"` or `"endfunction"`) from a leak diagnostic's `data`.
fn leak_kind(diag: &Diagnostic) -> Option<String> {
    diag.data.as_ref()?.get("leak_kind")?.as_str().map(String::from)
}

/// Extract leak type from a diagnostic's `data` field.
fn leak_type(diag: &Diagnostic) -> Option<String> {
    diag.data.as_ref()?.get("leak_type")?.as_str().map(String::from)
}

/// Extract function name from a diagnostic's `data` field.
fn leak_func_name(diag: &Diagnostic) -> Option<String> {
    diag.data.as_ref()?.get("func_name")?.as_str().map(String::from)
}

/// Check if the diagnostic is for a returned local variable.
fn is_returned_local(diag: &Diagnostic) -> bool {
    diag.data
        .as_ref()
        .and_then(|d| d.get("returned_local"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Find the line number of the first `endglobals` keyword in the rope.
/// Returns `None` if no globals block exists.
fn find_endglobals_line(rope: &lapce_xi_rope::Rope) -> Option<usize> {
    let line_count = rope.line_of_offset(rope.len()) + 1;
    for line in 0..line_count {
        let ls = rope.offset_of_line(line);
        let le = rope.offset_of_line(line + 1).min(rope.len());
        let text = rope.slice_to_cow(ls..le);
        if text.trim() == "endglobals" {
            return Some(line);
        }
    }
    None
}

/// Generate a unique global variable name `funcname_varname`, appending a
/// numeric suffix if the name already appears in the file text.
fn unique_global_name(func_name: &str, var_name: &str, rope: &lapce_xi_rope::Rope) -> String {
    let full_text = rope.slice_to_cow(0..rope.len());
    let base = format!("{}_{}", func_name, var_name);
    if !full_text.contains(&base) {
        return base;
    }
    let mut suffix = 1u32;
    loop {
        let candidate = format!("{}{}", base, suffix);
        if !full_text.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

/// Build text edits for the "returned local" leak fix.
///
/// 1. Insert a global variable declaration before `endglobals`
///    (or create a `globals`/`endglobals` block at line 0).
/// 2. Replace the `return <var>` line with:
///    ```
///    set <global> = <var>
///    set <var> = null
///    return <global>
///    ```
fn returned_local_edits(
    diag: &Diagnostic,
    rope: &lapce_xi_rope::Rope,
) -> Option<Vec<TextEdit>> {
    let var = leak_var(diag)?;
    let type_name = leak_type(diag)?;
    let func_name = leak_func_name(diag)?;
    let global_name = unique_global_name(&func_name, &var, rope);

    let mut edits = Vec::new();

    // ── 1. Insert global variable declaration ─────────────────────────────
    if let Some(endglobals_line) = find_endglobals_line(rope) {
        let glob_indent = body_indent(rope, endglobals_line);
        let insert_pos = Position { line: endglobals_line, character: 0 };
        edits.push(TextEdit {
            range: Range { start: insert_pos.clone(), end: insert_pos },
            new_text: format!("{}{} {}\n", glob_indent, type_name, global_name),
        });
    } else {
        // No globals block — create one at the top of the file.
        let insert_pos = Position { line: 0, character: 0 };
        edits.push(TextEdit {
            range: Range { start: insert_pos.clone(), end: insert_pos },
            new_text: format!("globals\n    {} {}\nendglobals\n\n", type_name, global_name),
        });
    }

    // ── 2. Replace the `return <var>` line ────────────────────────────────
    let ret_line = diag.range.start.line;
    let indent = line_indent(rope, ret_line);

    // Find the full extent of the return line.
    let line_count = rope.line_of_offset(rope.len()) + 1;
    let ret_line_start = rope.offset_of_line(ret_line);
    let ret_line_end = if ret_line + 1 < line_count {
        rope.offset_of_line(ret_line + 1)
    } else {
        rope.len()
    };
    let _ret_text = rope.slice_to_cow(ret_line_start..ret_line_end);

    let start_pos = Position { line: ret_line, character: 0 };
    let end_pos = Position { line: ret_line + 1, character: 0 };
    edits.push(TextEdit {
        range: Range { start: start_pos, end: end_pos },
        new_text: format!(
            "{indent}set {global} = {var}\n{indent}set {var} = null\n{indent}return {global}\n",
            indent = indent,
            global = global_name,
            var = var,
        ),
    });

    Some(edits)
}

/// Read the leading whitespace of a line in a rope.
fn line_indent(rope: &lapce_xi_rope::Rope, line: usize) -> String {
    let line_count = rope.line_of_offset(rope.len()) + 1;
    if line >= line_count {
        return String::new();
    }
    let line_start = rope.offset_of_line(line);
    let line_end = rope.offset_of_line(line + 1).min(rope.len());
    let text = rope.slice_to_cow(line_start..line_end);
    text.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// Find the body-level indentation for an `endfunction` leak.
/// Scans backwards from the `endfunction` line to find the first non-blank line
/// and uses its indentation.  Falls back to `endfunction` indent + 4 spaces.
fn body_indent(rope: &lapce_xi_rope::Rope, endfunction_line: usize) -> String {
    let line_count = rope.line_of_offset(rope.len()) + 1;
    let mut line = endfunction_line;
    while line > 0 {
        line -= 1;
        if line >= line_count {
            break;
        }
        let ls = rope.offset_of_line(line);
        let le = rope.offset_of_line(line + 1).min(rope.len());
        let text = rope.slice_to_cow(ls..le);
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return text
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();
        }
    }
    // Fallback: endfunction indent + 4 spaces.
    let ef_indent = line_indent(rope, endfunction_line);
    format!("{}    ", ef_indent)
}

/// Build a `TextEdit` that inserts `set <var> = null` before the given
/// diagnostic's target line, with appropriate indentation.
fn leak_text_edit(
    diag: &Diagnostic,
    rope: &lapce_xi_rope::Rope,
) -> Option<TextEdit> {
    let var = leak_var(diag)?;
    let kind = leak_kind(diag)?;
    let target_line = diag.range.start.line;

    let indent = if kind == "endfunction" {
        body_indent(rope, target_line)
    } else {
        // "return" — same indent as the return keyword line
        line_indent(rope, target_line)
    };

    let insert_pos = Position {
        line: target_line,
        character: 0,
    };

    Some(TextEdit {
        range: Range {
            start: insert_pos.clone(),
            end: insert_pos,
        },
        new_text: format!("{}set {} = null\n", indent, var),
    })
}

/// Compute quick fix actions for handle leak diagnostics.
fn compute_leak_fixes(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    let uri = &params.text_document.uri;
    let rope = match ROPE_MAP.get(uri) {
        Some(r) => r,
        None => return actions,
    };
    let rope = rope.value();

    // Collect leak diagnostics from the cursor context.
    let leak_diags: Vec<_> = params
        .context
        .diagnostics
        .iter()
        .filter(|d| d.has_code("leak"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    // ── Per-variable quick fixes ──────────────────────────────────────────
    // Group by (line, var) to avoid duplicate edits at the same location.
    let mut seen = std::collections::HashSet::new();
    for diag in &leak_diags {
        let var = match leak_var(diag) {
            Some(v) => v,
            None => continue,
        };
        let key = (diag.range.start.line, var.clone());
        if !seen.insert(key) {
            continue;
        }
        if is_returned_local(diag) {
            // Returned local: create global + rewrite return.
            if let Some(edits) = returned_local_edits(diag, rope) {
                let title = crate::util::i18n::fix_handle_leak(&var);
                let mut changes = HashMap::new();
                changes.insert(uri.clone(), edits);
                actions.push(CodeAction {
                    title,
                    kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                    }),
                    command: None,
                });
            }
        } else if let Some(edit) = leak_text_edit(diag, rope) {
            let title = crate::util::i18n::fix_handle_leak(&var);
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![edit]);
            actions.push(CodeAction {
                title,
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                }),
                command: None,
            });
        }
    }

    // ── Fix all leaks in file ─────────────────────────────────────────────
    if !leak_diags.is_empty() {
        if let Some(file_action) = compute_fix_all_leaks(uri, rope) {
            actions.push(file_action);
        }
    }

    actions
}

/// Build a single code action that fixes ALL handle leaks in the file.
fn compute_fix_all_leaks(
    uri: &url::Url,
    rope: &lapce_xi_rope::Rope,
) -> Option<CodeAction> {
    // Get all diagnostics for this file from FILE_STORE.
    let snap = FILE_STORE.get(uri)?;
    let all_diags = &snap.value().diagnostics;

    let leak_diags: Vec<_> = all_diags
        .iter()
        .filter(|d| d.has_code("leak"))
        .filter(|d| d.data.is_some())
        .collect();

    if leak_diags.len() < 2 {
        return None;
    }

    // Build edits, deduplicated by (line, var), sorted by line descending
    // so that insertions don't shift line numbers of subsequent edits.
    let mut seen = std::collections::HashSet::new();
    let mut edits: Vec<(usize, TextEdit)> = Vec::new();
    for diag in &leak_diags {
        let var = match leak_var(diag) {
            Some(v) => v,
            None => continue,
        };
        let key = (diag.range.start.line, var);
        if !seen.insert(key) {
            continue;
        }
        if is_returned_local(diag) {
            if let Some(ret_edits) = returned_local_edits(diag, rope) {
                for e in ret_edits {
                    edits.push((diag.range.start.line, e));
                }
            }
        } else if let Some(edit) = leak_text_edit(diag, rope) {
            edits.push((diag.range.start.line, edit));
        }
    }

    // Sort by line descending so inserts don't invalidate positions.
    edits.sort_by(|a, b| b.0.cmp(&a.0));
    let text_edits: Vec<TextEdit> = edits.into_iter().map(|(_, e)| e).collect();

    if text_edits.is_empty() {
        return None;
    }

    let title = crate::util::i18n::fix_all_handle_leaks().to_string();
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), text_edits);

    Some(CodeAction {
        title,
        kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
        }),
        command: None,
    })
}

// ─── Unused function fixes ───────────────────────────────────────────────────

/// Quick-fix and "fix all" actions for unused function diagnostics.
fn compute_unused_func_fixes(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();
    let uri = &params.text_document.uri;
    let rope = match ROPE_MAP.get(uri) {
        Some(r) => r,
        None => return actions,
    };
    let rope = rope.value();

    let diags: Vec<_> = params
        .context
        .diagnostics
        .iter()
        .filter(|d| d.has_code("unused-function"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    for diag in &diags {
        if let Some(func_range) = diag
            .data
            .as_ref()
            .and_then(|d| d.get("unused_func_range"))
            .and_then(|v| serde_json::from_value::<Range>(v.clone()).ok())
        {
            let edit = func_delete_edit(rope, &func_range);
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![edit]);
            actions.push(CodeAction {
                title: crate::util::i18n::remove_unused_function().to_string(),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit { changes: Some(changes) }),
                command: None,
            });
        }
    }

    if !diags.is_empty() {
        if let Some(file_action) = compute_unused_func_fix_all(uri) {
            actions.push(file_action);
        }
    }

    actions
}

/// Build a single code action that removes ALL unused functions in the file.
fn compute_unused_func_fix_all(uri: &url::Url) -> Option<CodeAction> {
    let snap = FILE_STORE.get(uri)?;
    let all_diags = &snap.value().diagnostics;

    let unused_diags: Vec<_> = all_diags
        .iter()
        .filter(|d| d.has_code("unused-function"))
        .filter(|d| d.data.is_some())
        .collect();

    if unused_diags.len() < 2 {
        return None;
    }

    let rope = ROPE_MAP.get(uri)?;
    let rope = rope.value();

    let mut edits: Vec<(usize, usize, TextEdit)> = Vec::new();
    for diag in &unused_diags {
        if let Some(func_range) = diag
            .data
            .as_ref()
            .and_then(|d| d.get("unused_func_range"))
            .and_then(|v| serde_json::from_value::<Range>(v.clone()).ok())
        {
            let edit = func_delete_edit(rope, &func_range);
            edits.push((
                edit.range.start.line,
                edit.range.start.character,
                edit,
            ));
        }
    }

    if edits.is_empty() {
        return None;
    }

    // Sort descending so deletions don't shift earlier positions.
    edits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let text_edits: Vec<TextEdit> = edits.into_iter().map(|(_, _, e)| e).collect();

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), text_edits);

    Some(CodeAction {
        title: crate::util::i18n::remove_all_unused_functions().to_string(),
        kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
        diagnostics: None,
        edit: Some(WorkspaceEdit { changes: Some(changes) }),
        command: None,
    })
}

// ─── Inline single-call function fixes ───────────────────────────────────────

/// Extract inline metadata from a diagnostic's `data` field.
fn inline_data(diag: &Diagnostic) -> Option<(String, String, bool, Range)> {
    let data = diag.data.as_ref()?;
    let name = data.get("inline_name")?.as_str()?.to_string();
    let expr = data.get("inline_expr")?.as_str()?.to_string();
    let is_compound = data.get("inline_is_compound")?.as_bool().unwrap_or(false);
    let func_range: Range = serde_json::from_value(data.get("inline_func_range")?.clone()).ok()?;
    Some((name, expr, is_compound, func_range))
}

/// Build a `TextEdit` that deletes the entire function declaration,
/// extending the range to consume the trailing newline (if present).
fn func_delete_edit(rope: &lapce_xi_rope::Rope, func_range: &Range) -> TextEdit {
    let end_line = func_range.end.line;
    let line_count = rope.line_of_offset(rope.len()) + 1;
    // Extend to the start of the next line to consume the trailing newline.
    let end = if end_line + 1 < line_count {
        Position { line: end_line + 1, character: 0 }
    } else {
        func_range.end.clone()
    };
    // Also consume a preceding blank line if the function starts at line > 0.
    let start = func_range.start.clone();
    TextEdit {
        range: Range { start, end },
        new_text: String::new(),
    }
}

/// Check whether `NAME()` at `call_start..call_end` in `source` is a top-level
/// expression (the sole expression in its syntactic slot).
fn is_top_level_call_in_text(source: &str, call_start: usize, call_end: usize) -> bool {
    let line_start = source[..call_start].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_end = source[call_end..].find('\n').map(|p| call_end + p).unwrap_or(source.len());

    let before = source[line_start..call_start].trim();
    let after = source[call_end..line_end].trim();

    // `call NAME()`
    if before.ends_with("call") && after.is_empty() { return true; }
    // `return NAME()`
    if before.ends_with("return") && after.is_empty() { return true; }
    // `exitwhen NAME()`
    if before.ends_with("exitwhen") && after.is_empty() { return true; }
    // `set VAR = NAME()` / `set VAR[IDX] = NAME()`
    if before.starts_with("set ") && before.ends_with('=') && after.is_empty() { return true; }
    // `if NAME() then` / `elseif NAME() then`
    if before.ends_with("if") && after == "then" { return true; }

    false
}

/// Find `NAME()` in the rope text (respecting word boundaries) and build
/// a `TextEdit` that replaces it with the inlined expression.
///
/// Returns `Some((edit, byte_offset_of_match))` on success.
fn find_call_and_build_edit(
    rope: &lapce_xi_rope::Rope,
    func_name: &str,
    expr: &str,
    is_compound: bool,
) -> Option<TextEdit> {
    let source = rope.slice_to_cow(0..rope.len());
    let source = source.as_ref();
    let pattern = format!("{}()", func_name);
    let mut search_from = 0;

    while let Some(pos) = source[search_from..].find(&pattern) {
        let abs_pos = search_from + pos;
        let is_boundary = if abs_pos == 0 {
            true
        } else {
            let b = source.as_bytes()[abs_pos - 1];
            !b.is_ascii_alphanumeric() && b != b'_'
        };

        if is_boundary {
            let call_end = abs_pos + pattern.len();
            let top_level = is_top_level_call_in_text(source, abs_pos, call_end);
            let replacement = if top_level || !is_compound {
                expr.to_string()
            } else {
                format!("({})", expr)
            };
            let range = Range::from_byte_offsets(rope, abs_pos, call_end);
            return Some(TextEdit { range, new_text: replacement });
        }

        search_from = abs_pos + pattern.len();
    }

    None
}

/// Build edits (func deletion + call replacement) for a single inline diagnostic.
fn build_inline_edits(
    uri: &url::Url,
    func_name: &str,
    expr: &str,
    is_compound: bool,
    func_range: &Range,
) -> Option<HashMap<url::Url, Vec<TextEdit>>> {
    let rope = ROPE_MAP.get(uri)?;
    let rope = rope.value();

    let delete_edit = func_delete_edit(rope, func_range);

    // Search current file for the call site.
    if let Some(replace_edit) = find_call_and_build_edit(rope, func_name, expr, is_compound) {
        // Both edits are in the same file — make sure delete comes after replace
        // in the list (sorted descending) so positions stay valid.
        let mut edits = vec![delete_edit, replace_edit];
        edits.sort_by(|a, b| b.range.start.line.cmp(&a.range.start.line)
            .then(b.range.start.character.cmp(&a.range.start.character)));
        let mut changes = HashMap::new();
        changes.insert(uri.clone(), edits);
        return Some(changes);
    }

    // Search other files in the visible component for the call site.
    let component = crate::util::import_graph::IMPORT_GRAPH.visible_component(uri);
    for peer_uri in &component {
        if peer_uri == uri { continue; }
        if let Some(peer_rope) = ROPE_MAP.get(peer_uri) {
            let peer_rope = peer_rope.value();
            if let Some(replace_edit) = find_call_and_build_edit(peer_rope, func_name, expr, is_compound) {
                let mut changes = HashMap::new();
                changes.insert(uri.clone(), vec![delete_edit]);
                changes.insert(peer_uri.clone(), vec![replace_edit]);
                return Some(changes);
            }
        }
    }

    None
}

/// Quick-fix and "fix all" actions for inlinable function diagnostics.
fn compute_inline_fixes(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();
    let uri = &params.text_document.uri;

    let inline_diags: Vec<_> = params
        .context
        .diagnostics
        .iter()
        .filter(|d| d.has_code("inline"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    for diag in &inline_diags {
        if let Some((name, expr, is_compound, func_range)) = inline_data(diag) {
            if let Some(changes) = build_inline_edits(uri, &name, &expr, is_compound, &func_range) {
                actions.push(CodeAction {
                    title: crate::util::i18n::inline_function_action().to_string(),
                    kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit { changes: Some(changes) }),
                    command: None,
                });
            }
        }
    }

    // "Fix all" action — inline all single-call functions in the file.
    if !inline_diags.is_empty() {
        if let Some(file_action) = compute_inline_fix_all(uri) {
            actions.push(file_action);
        }
    }

    actions
}

/// Build a single code action that inlines ALL single-call functions in the file.
///
/// Handles nested inline functions: if inline function A's expression calls
/// inline function B, B's expression is recursively resolved into A's before
/// the final call-site replacement.  All inline function declarations are
/// deleted, and only call sites *outside* any inline function body get a
/// replacement edit.
fn compute_inline_fix_all(uri: &url::Url) -> Option<CodeAction> {
    let snap = FILE_STORE.get(uri)?;
    let all_diags = &snap.value().diagnostics;

    let inline_diags: Vec<_> = all_diags
        .iter()
        .filter(|d| d.has_code("inline"))
        .filter(|d| d.data.is_some())
        .collect();

    if inline_diags.len() < 2 {
        return None;
    }

    // Step 1: Collect all inline candidates.
    //         name → (expr_text, is_compound, func_range)
    let mut inline_map: HashMap<String, (String, bool, Range)> = HashMap::new();
    for diag in &inline_diags {
        if let Some((name, expr, is_compound, func_range)) = inline_data(diag) {
            inline_map.insert(name, (expr, is_compound, func_range));
        }
    }
    if inline_map.len() < 2 {
        return None;
    }

    // Step 2: Build dependency graph.
    //         deps[name] = list of other inline functions called by `name`.
    let names: Vec<String> = inline_map.keys().cloned().collect();
    let mut deps: HashMap<String, Vec<String>> = HashMap::new();
    for (name, (expr, _, _)) in &inline_map {
        let mut name_deps = Vec::new();
        for other in &names {
            if other != name && has_call_in_text(expr, other) {
                name_deps.push(other.clone());
            }
        }
        deps.insert(name.clone(), name_deps);
    }

    // Step 3: Topological sort (leaves first, dependents last).
    let sorted = topo_sort_inline(&names, &deps);

    // Step 4: Resolve expressions bottom-up.
    //         When processing a function, all its inline-callee expressions
    //         are already resolved, so a single pass suffices.
    let mut resolved_exprs: HashMap<String, String> = HashMap::new();
    for name in &sorted {
        let (expr, _, _) = &inline_map[name];
        let mut resolved = expr.clone();
        for dep in deps.get(name).into_iter().flatten() {
            let dep_resolved = &resolved_exprs[dep];
            let (_, dep_compound, _) = &inline_map[dep];
            resolved = replace_call_in_text(&resolved, dep, dep_resolved, *dep_compound);
        }
        resolved_exprs.insert(name.clone(), resolved);
    }

    // Step 5: Build edits.
    let rope = ROPE_MAP.get(uri)?;
    let rope = rope.value();
    let mut edits: Vec<TextEdit> = Vec::new();

    // 5a. Delete all inline function declarations.
    for (_, _, func_range) in inline_map.values() {
        edits.push(func_delete_edit(rope, func_range));
    }

    // 5b. Replace call sites that are OUTSIDE all inline function bodies.
    let exclude_ranges: Vec<&Range> = inline_map.values().map(|(_, _, r)| r).collect();
    for (name, (_, is_compound, _)) in &inline_map {
        let resolved_expr = &resolved_exprs[name];
        if let Some(call_edit) = find_call_and_build_edit_excluding(
            rope, name, resolved_expr, *is_compound, &exclude_ranges,
        ) {
            edits.push(call_edit);
        } else {
            // Call site might be in another file.
            let component = crate::util::import_graph::IMPORT_GRAPH.visible_component(uri);
            for peer_uri in &component {
                if peer_uri == uri { continue; }
                if let Some(peer_rope) = ROPE_MAP.get(peer_uri) {
                    let peer_rope = peer_rope.value();
                    if let Some(call_edit) = find_call_and_build_edit(
                        peer_rope, name, resolved_expr, *is_compound,
                    ) {
                        // Cross-file edits are not supported in "fix all" yet.
                        // For now, skip.  (Single-function inline handles this.)
                        let _ = call_edit;
                    }
                }
            }
        }
    }

    if edits.is_empty() {
        return None;
    }

    // Sort descending so edits don't invalidate each other's positions.
    edits.sort_by(|a, b| b.range.start.line.cmp(&a.range.start.line)
        .then(b.range.start.character.cmp(&a.range.start.character)));

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);

    Some(CodeAction {
        title: crate::util::i18n::inline_all_functions_action().to_string(),
        kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
        diagnostics: None,
        edit: Some(WorkspaceEdit { changes: Some(changes) }),
        command: None,
    })
}

// ─── Inline helpers ──────────────────────────────────────────────────────────

/// Check whether `source` contains a word-boundary `NAME()` call.
fn has_call_in_text(source: &str, func_name: &str) -> bool {
    let pattern = format!("{}()", func_name);
    let mut search_from = 0;
    while let Some(pos) = source[search_from..].find(&pattern) {
        let abs_pos = search_from + pos;
        let is_boundary = if abs_pos == 0 {
            true
        } else {
            let b = source.as_bytes()[abs_pos - 1];
            !b.is_ascii_alphanumeric() && b != b'_'
        };
        if is_boundary {
            return true;
        }
        search_from = abs_pos + pattern.len();
    }
    false
}

/// Replace all word-boundary `NAME()` occurrences in `source` with
/// `replacement` (wrapped in parentheses if compound and not top-level).
fn replace_call_in_text(
    source: &str,
    func_name: &str,
    replacement: &str,
    is_compound: bool,
) -> String {
    let pattern = format!("{}()", func_name);
    let mut result = String::with_capacity(source.len());
    let mut search_from = 0;

    while let Some(pos) = source[search_from..].find(&pattern) {
        let abs_pos = search_from + pos;
        let is_boundary = if abs_pos == 0 {
            true
        } else {
            let b = source.as_bytes()[abs_pos - 1];
            !b.is_ascii_alphanumeric() && b != b'_'
        };

        if !is_boundary {
            result.push_str(&source[search_from..abs_pos + pattern.len()]);
            search_from = abs_pos + pattern.len();
            continue;
        }

        let call_end = abs_pos + pattern.len();
        let top_level = is_top_level_call_in_text(source, abs_pos, call_end);

        result.push_str(&source[search_from..abs_pos]);
        if top_level || !is_compound {
            result.push_str(replacement);
        } else {
            result.push('(');
            result.push_str(replacement);
            result.push(')');
        }

        search_from = call_end;
    }

    result.push_str(&source[search_from..]);
    result
}

/// DFS-based topological sort: dependencies (leaves) come before dependents.
fn topo_sort_inline(
    names: &[String],
    deps: &HashMap<String, Vec<String>>,
) -> Vec<String> {
    let mut visited = std::collections::HashSet::new();
    let mut order = Vec::new();

    fn dfs(
        name: &str,
        deps: &HashMap<String, Vec<String>>,
        visited: &mut std::collections::HashSet<String>,
        order: &mut Vec<String>,
    ) {
        if visited.contains(name) { return; }
        visited.insert(name.to_string());
        if let Some(d) = deps.get(name) {
            for dep in d {
                dfs(dep, deps, visited, order);
            }
        }
        order.push(name.to_string());
    }

    // Alphabetical seed order for determinism.
    let mut sorted_names: Vec<&String> = names.iter().collect();
    sorted_names.sort();
    for name in sorted_names {
        dfs(name, deps, &mut visited, &mut order);
    }

    order
}

/// Like [`find_call_and_build_edit`] but skips matches whose line falls
/// inside any of the `exclude_ranges` (used to skip calls inside inline
/// function bodies that are being deleted).
fn find_call_and_build_edit_excluding(
    rope: &lapce_xi_rope::Rope,
    func_name: &str,
    expr: &str,
    is_compound: bool,
    exclude_ranges: &[&Range],
) -> Option<TextEdit> {
    let source = rope.slice_to_cow(0..rope.len());
    let source = source.as_ref();
    let pattern = format!("{}()", func_name);
    let mut search_from = 0;

    while let Some(pos) = source[search_from..].find(&pattern) {
        let abs_pos = search_from + pos;
        let is_boundary = if abs_pos == 0 {
            true
        } else {
            let b = source.as_bytes()[abs_pos - 1];
            !b.is_ascii_alphanumeric() && b != b'_'
        };

        if is_boundary {
            let call_end = abs_pos + pattern.len();
            let range = Range::from_byte_offsets(rope, abs_pos, call_end);
            let call_line = range.start.line;

            // Skip if the call is inside an excluded (deleted) function.
            let excluded = exclude_ranges.iter().any(|r| {
                call_line >= r.start.line && call_line <= r.end.line
            });

            if !excluded {
                let top_level = is_top_level_call_in_text(source, abs_pos, call_end);
                let replacement = if top_level || !is_compound {
                    expr.to_string()
                } else {
                    format!("({})", expr)
                };
                return Some(TextEdit { range, new_text: replacement });
            }
        }

        search_from = abs_pos + pattern.len();
    }

    None
}

/// Quick-fix and "fix all" actions for `if not(cond) then return false endif`
/// chains that can be collapsed into a single `return … and … and …`.
fn compute_collapse_and_fixes(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    let uri = &params.text_document.uri;
    let _rope = match ROPE_MAP.get(uri) {
        Some(r) => r,
        None => return actions,
    };

    let collapse_diags: Vec<_> = params
        .context
        .diagnostics
        .iter()
        .filter(|d| d.has_code("collapse-and"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    for diag in &collapse_diags {
        if let Some(new_text) = diag
            .data
            .as_ref()
            .and_then(|d| d.get("collapse_and_new_text"))
            .and_then(|v| v.as_str())
        {
            let edit = TextEdit {
                range: diag.range.clone(),
                new_text: new_text.to_string(),
            };
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![edit]);
            actions.push(CodeAction {
                title: crate::util::i18n::collapse_and_chain_action().to_string(),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit { changes: Some(changes) }),
                command: None,
            });
        }
    }

    // "Fix all" action (needs ≥ 2 collapse_and patterns in the file).
    if !collapse_diags.is_empty() {
        if let Some(file_action) = compute_collapse_and_fix_all(uri) {
            actions.push(file_action);
        }
    }

    actions
}

/// Build a single code action that collapses ALL and-chains in the file.
fn compute_collapse_and_fix_all(uri: &url::Url) -> Option<CodeAction> {
    let snap = FILE_STORE.get(uri)?;
    let all_diags = &snap.value().diagnostics;

    let collapse_diags: Vec<_> = all_diags
        .iter()
        .filter(|d| d.has_code("collapse-and"))
        .filter(|d| d.data.is_some())
        .collect();

    if collapse_diags.len() < 2 {
        return None;
    }

    let mut edits: Vec<(usize, usize, TextEdit)> = Vec::new();
    for diag in &collapse_diags {
        if let Some(new_text) = diag
            .data
            .as_ref()
            .and_then(|d| d.get("collapse_and_new_text"))
            .and_then(|v| v.as_str())
        {
            edits.push((
                diag.range.start.line,
                diag.range.start.character,
                TextEdit {
                    range: diag.range.clone(),
                    new_text: new_text.to_string(),
                },
            ));
        }
    }

    edits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let text_edits: Vec<TextEdit> = edits.into_iter().map(|(_, _, e)| e).collect();

    if text_edits.is_empty() {
        return None;
    }

    let title = crate::util::i18n::collapse_all_and_chains_action().to_string();
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), text_edits);

    Some(CodeAction {
        title,
        kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
        diagnostics: None,
        edit: Some(WorkspaceEdit { changes: Some(changes) }),
        command: None,
    })
}

// ─── Collapse or-chain fixes ─────────────────────────────────────────────────

/// Quick-fix and "fix all" actions for `if (cond) then return true endif`
/// chains that can be collapsed into a single `return … or … or …`.
fn compute_collapse_or_fixes(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    let uri = &params.text_document.uri;
    let _rope = match ROPE_MAP.get(uri) {
        Some(r) => r,
        None => return actions,
    };

    let collapse_diags: Vec<_> = params
        .context
        .diagnostics
        .iter()
        .filter(|d| d.has_code("collapse-or"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    for diag in &collapse_diags {
        if let Some(new_text) = diag
            .data
            .as_ref()
            .and_then(|d| d.get("collapse_or_new_text"))
            .and_then(|v| v.as_str())
        {
            let edit = TextEdit {
                range: diag.range.clone(),
                new_text: new_text.to_string(),
            };
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![edit]);
            actions.push(CodeAction {
                title: crate::util::i18n::collapse_or_chain_action().to_string(),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit { changes: Some(changes) }),
                command: None,
            });
        }
    }

    // "Fix all" action (needs ≥ 2 collapse_or patterns in the file).
    if !collapse_diags.is_empty() {
        if let Some(file_action) = compute_collapse_or_fix_all(uri) {
            actions.push(file_action);
        }
    }

    actions
}

/// Build a single code action that collapses ALL or-chains in the file.
fn compute_collapse_or_fix_all(uri: &url::Url) -> Option<CodeAction> {
    let snap = FILE_STORE.get(uri)?;
    let all_diags = &snap.value().diagnostics;

    let collapse_diags: Vec<_> = all_diags
        .iter()
        .filter(|d| d.has_code("collapse-or"))
        .filter(|d| d.data.is_some())
        .collect();

    if collapse_diags.len() < 2 {
        return None;
    }

    let mut edits: Vec<(usize, usize, TextEdit)> = Vec::new();
    for diag in &collapse_diags {
        if let Some(new_text) = diag
            .data
            .as_ref()
            .and_then(|d| d.get("collapse_or_new_text"))
            .and_then(|v| v.as_str())
        {
            edits.push((
                diag.range.start.line,
                diag.range.start.character,
                TextEdit {
                    range: diag.range.clone(),
                    new_text: new_text.to_string(),
                },
            ));
        }
    }

    edits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let text_edits: Vec<TextEdit> = edits.into_iter().map(|(_, _, e)| e).collect();

    if text_edits.is_empty() {
        return None;
    }

    let title = crate::util::i18n::collapse_all_or_chains_action().to_string();
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), text_edits);

    Some(CodeAction {
        title,
        kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
        diagnostics: None,
        edit: Some(WorkspaceEdit { changes: Some(changes) }),
        command: None,
    })
}

// ─── Empty else fixes ────────────────────────────────────────────────────────

/// Quick-fix and "fix all" actions for empty `else` blocks.
fn compute_empty_else_fixes(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();
    let uri = &params.text_document.uri;

    let diags: Vec<_> = params
        .context
        .diagnostics
        .iter()
        .filter(|d| d.has_code("empty-else"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    for diag in &diags {
        if let Some(delete_range) = diag
            .data
            .as_ref()
            .and_then(|d| d.get("empty_else_delete_range"))
            .and_then(|v| serde_json::from_value::<Range>(v.clone()).ok())
        {
            let edit = TextEdit {
                range: delete_range,
                new_text: String::new(),
            };
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![edit]);
            actions.push(CodeAction {
                title: crate::util::i18n::remove_empty_else().to_string(),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit { changes: Some(changes) }),
                command: None,
            });
        }
    }

    if !diags.is_empty() {
        if let Some(file_action) = compute_empty_else_fix_all(uri) {
            actions.push(file_action);
        }
    }

    actions
}

/// Build a single code action that removes ALL empty else blocks in the file.
fn compute_empty_else_fix_all(uri: &url::Url) -> Option<CodeAction> {
    let snap = FILE_STORE.get(uri)?;
    let all_diags = &snap.value().diagnostics;

    let empty_else_diags: Vec<_> = all_diags
        .iter()
        .filter(|d| d.has_code("empty-else"))
        .filter(|d| d.data.is_some())
        .collect();

    if empty_else_diags.len() < 2 {
        return None;
    }

    let mut edits: Vec<(usize, usize, TextEdit)> = Vec::new();
    for diag in &empty_else_diags {
        if let Some(delete_range) = diag
            .data
            .as_ref()
            .and_then(|d| d.get("empty_else_delete_range"))
            .and_then(|v| serde_json::from_value::<Range>(v.clone()).ok())
        {
            edits.push((
                delete_range.start.line,
                delete_range.start.character,
                TextEdit {
                    range: delete_range,
                    new_text: String::new(),
                },
            ));
        }
    }

    if edits.is_empty() {
        return None;
    }

    // Sort descending so later deletions don't shift earlier positions.
    edits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let text_edits: Vec<TextEdit> = edits.into_iter().map(|(_, _, e)| e).collect();

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), text_edits);

    Some(CodeAction {
        title: crate::util::i18n::remove_all_empty_else().to_string(),
        kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
        diagnostics: None,
        edit: Some(WorkspaceEdit { changes: Some(changes) }),
        command: None,
    })
}

// ─── Remove else branch refactoring ──────────────────────────────────────────

const JASS_ELSE_KIND: u16 = crate::lng::jass::kind::Kind::Else as u16;
const JASS_ENDIF_KIND: u16 = crate::lng::jass::kind::Kind::Endif as u16;
const JASS_IF_STATEMENT_KIND: u16 = crate::lng::jass::kind::Kind::IfStatement as u16;

/// Position-based refactoring: if the cursor is on an `else` keyword, offer
/// to remove the else branch.
///
/// The edit:
///   1. Replaces `else` with `endif` (same indentation).
///   2. Removes the old `endif` line.
///   3. De-indents the old else body so it aligns with the new `endif`.
fn compute_remove_else_action(params: &CodeActionParams) -> Option<CodeAction> {
    let uri = &params.text_document.uri;
    let rope = ROPE_MAP.get(uri)?;
    let rope = rope.value();
    let tree = TREE_MAP.get(uri)?;
    let tree = tree.value();

    let point = Point {
        row: params.range.start.line,
        column: params.range.start.character,
    };

    let root = tree.root_node();
    let node = root.descendant_for_point_range(point, point)?;

    // Check if we're on an `else` keyword.
    if node.grammar_id() != JASS_ELSE_KIND {
        return None;
    }

    // The parent must be an `if_statement`.
    let if_stmt = node.parent()?;
    if if_stmt.grammar_id() != JASS_IF_STATEMENT_KIND {
        return None;
    }

    // Find the `endif` keyword among siblings after `else`.
    let mut endif_node = None;
    let mut sib = node.next_sibling();
    while let Some(n) = sib {
        if n.grammar_id() == JASS_ENDIF_KIND {
            endif_node = Some(n);
            break;
        }
        sib = n.next_sibling();
    }
    let endif_node = endif_node?;

    let else_line = node.start_position().row;
    let endif_line = endif_node.start_position().row;

    let else_indent = line_indent(rope, else_line);

    // ── Build replacement text ──────────────────────────────────────────
    let mut new_text = format!("{}endif\n", else_indent);

    let body_start = else_line + 1;
    let body_end = endif_line; // exclusive

    if body_start < body_end {
        // Find minimum indentation among non-empty body lines.
        let line_count = rope.line_of_offset(rope.len()) + 1;
        let mut min_indent = usize::MAX;
        for line_num in body_start..body_end {
            if line_num >= line_count {
                break;
            }
            let ls = rope.offset_of_line(line_num);
            let le = rope.offset_of_line(line_num + 1).min(rope.len());
            let text = rope.slice_to_cow(ls..le);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            let indent_len = text
                .bytes()
                .take_while(|b| *b == b' ' || *b == b'\t')
                .count();
            min_indent = min_indent.min(indent_len);
        }

        let excess = if min_indent == usize::MAX {
            0
        } else {
            min_indent.saturating_sub(else_indent.len())
        };

        for line_num in body_start..body_end {
            if line_num >= line_count {
                break;
            }
            let ls = rope.offset_of_line(line_num);
            let le = rope.offset_of_line(line_num + 1).min(rope.len());
            let text = rope.slice_to_cow(ls..le);
            let text_ref = text.as_ref();

            // Strip `excess` leading whitespace bytes.
            let leading_ws = text_ref
                .bytes()
                .take_while(|b| *b == b' ' || *b == b'\t')
                .count();
            let to_strip = excess.min(leading_ws);
            if to_strip > 0 {
                new_text.push_str(&text_ref[to_strip..]);
            }
        }
    }

    // ── Edit range: from start of `else` line to end of `endif` line ────
    let edit_start = rope.offset_of_line(else_line);
    let line_count = rope.line_of_offset(rope.len()) + 1;
    let edit_end = if endif_line + 1 < line_count {
        rope.offset_of_line(endif_line + 1)
    } else {
        rope.len()
    };

    let range = Range::from_byte_offsets(rope, edit_start, edit_end);

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![TextEdit { range, new_text }]);

    Some(CodeAction {
        title: crate::util::i18n::remove_else_branch().to_string(),
        kind: Some(CODE_ACTION_KIND_REFACTOR.into()),
        diagnostics: None,
        edit: Some(WorkspaceEdit { changes: Some(changes) }),
        command: None,
    })
}

// ─── ExecuteFunc quick fixes ─────────────────────────────────────────────────

/// Quick-fix and "fix all" actions for `ExecuteFunc("FuncName")` → `call FuncName()`.
fn compute_execute_func_fixes(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    let uri = &params.text_document.uri;

    // Per-diagnostic quick fixes from the current request context.
    let exec_diags: Vec<_> = params
        .context
        .diagnostics
        .iter()
        .filter(|d| d.has_code("execute-func"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    for diag in &exec_diags {
        if let Some(new_text) = diag
            .data
            .as_ref()
            .and_then(|d| d.get("execute_func_new_text"))
            .and_then(|v| v.as_str())
        {
            // Extract function name for the title (strip "call " and "()")
            let func_name = new_text
                .strip_prefix("call ")
                .and_then(|s| s.strip_suffix("()"))
                .unwrap_or(new_text);

            let edit = TextEdit {
                range: diag.range.clone(),
                new_text: new_text.to_string(),
            };
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![edit]);
            actions.push(CodeAction {
                title: crate::util::i18n::execute_func_replace(func_name),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit { changes: Some(changes) }),
                command: None,
            });
        }
    }

    // "Fix all" action (needs ≥ 2 fixable patterns in the file).
    if !exec_diags.is_empty() {
        if let Some(file_action) = compute_execute_func_fix_all(uri) {
            actions.push(file_action);
        }
    }

    actions
}

/// Build a single code action that fixes ALL `ExecuteFunc` calls in the file.
fn compute_execute_func_fix_all(uri: &url::Url) -> Option<CodeAction> {
    let snap = FILE_STORE.get(uri)?;
    let all_diags = &snap.value().diagnostics;

    let exec_diags: Vec<_> = all_diags
        .iter()
        .filter(|d| d.has_code("execute-func"))
        .filter(|d| d.data.is_some())
        .collect();

    if exec_diags.len() < 2 {
        return None;
    }

    let mut edits: Vec<(usize, usize, TextEdit)> = Vec::new();
    for diag in &exec_diags {
        if let Some(new_text) = diag
            .data
            .as_ref()
            .and_then(|d| d.get("execute_func_new_text"))
            .and_then(|v| v.as_str())
        {
            edits.push((
                diag.range.start.line,
                diag.range.start.character,
                TextEdit {
                    range: diag.range.clone(),
                    new_text: new_text.to_string(),
                },
            ));
        }
    }

    edits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let text_edits: Vec<TextEdit> = edits.into_iter().map(|(_, _, e)| e).collect();

    if text_edits.is_empty() {
        return None;
    }

    let title = crate::util::i18n::execute_func_replace_all().to_string();
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), text_edits);

    Some(CodeAction {
        title,
        kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
        diagnostics: None,
        edit: Some(WorkspaceEdit { changes: Some(changes) }),
        command: None,
    })
}

// ─── Fold StringHash ─────────────────────────────────────────────────────────

use crate::lng::jass::kind::{Field, Kind};
use crate::util::string_hash::{
    blizzard_string_hash, collect_constants, eval_const_expr, ConstValue,
};

const FIELD_NAME: u16 = Field::Name as u16;
const FIELD_ARGS: u16 = Field::Args as u16;

/// Info about a single foldable site in the file.
struct StringHashSite {
    /// Byte range to replace.
    start: usize,
    end: usize,
    /// The evaluated hash value.
    hash: i32,
}

/// Build a signature map: `func_name → [param_type, …]` from all known functions/natives.
fn build_signature_map_for_uri(uri: &url::Url) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    // Collect from the file itself and all connected files.
    let component = crate::util::import_graph::IMPORT_GRAPH.visible_component(uri);
    for file_uri in &component {
        if let Some(entry) = FILE_STORE.get(file_uri) {
            let symbols = &entry.value().file_symbols;
            for f in &symbols.functions {
                let types: Vec<String> = f.params.iter().map(|p| p.type_name.clone()).collect();
                map.insert(f.name.clone(), types);
            }
            for n in &symbols.natives {
                let types: Vec<String> = n.params.iter().map(|p| p.type_name.clone()).collect();
                map.insert(n.name.clone(), types);
            }
        }
    }
    // Also check the file itself in case it's not yet in the graph.
    if let Some(entry) = FILE_STORE.get(uri) {
        let symbols = &entry.value().file_symbols;
        for f in &symbols.functions {
            let types: Vec<String> = f.params.iter().map(|p| p.type_name.clone()).collect();
            map.entry(f.name.clone()).or_insert(types);
        }
        for n in &symbols.natives {
            let types: Vec<String> = n.params.iter().map(|p| p.type_name.clone()).collect();
            map.entry(n.name.clone()).or_insert(types);
        }
    }
    map
}

/// Compute code actions for folding `StringHash(expr)` → integer constant
/// and for replacing string arguments in integer parameter positions.
fn compute_string_hash_fold(params: &CodeActionParams) -> Vec<CodeAction> {
    let uri = &params.text_document.uri;
    let rope = match ROPE_MAP.get(uri) {
        Some(r) => r,
        None => return vec![],
    };
    let tree = match TREE_MAP.get(uri) {
        Some(t) => t,
        None => return vec![],
    };

    // Collect constant values from globals in the file text.
    let text = rope.to_string();
    let const_lines: Vec<String> = text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("constant "))
        .collect();
    let constants = collect_constants(&const_lines);

    // Build function signature map.
    let signatures = build_signature_map_for_uri(uri);

    // Walk the tree and collect all foldable sites.
    let root = tree.root_node();
    let mut sites: Vec<StringHashSite> = Vec::new();
    collect_string_hash_sites(&root, &text, &constants, &signatures, &mut sites);

    if sites.is_empty() {
        return vec![];
    }

    let mut actions = Vec::new();

    // Find the site under the cursor.
    let cursor_byte = rope.offset_of_line(params.range.start.line) + params.range.start.character;
    if let Some(site) = sites.iter().find(|s| cursor_byte >= s.start && cursor_byte <= s.end) {
        let range = Range::from_byte_offsets(&rope, site.start, site.end);
        let mut changes = HashMap::new();
        changes.insert(uri.clone(), vec![TextEdit {
            range,
            new_text: site.hash.to_string(),
        }]);
        actions.push(CodeAction {
            title: crate::util::i18n::fold_string_hash().to_string(),
            kind: Some(CODE_ACTION_KIND_REFACTOR.into()),
            diagnostics: None,
            edit: Some(WorkspaceEdit { changes: Some(changes) }),
            command: None,
        });
    }

    // File-wide: fold all sites — only when the cursor is on a site
    // (single action was added) and there are more sites in the file.
    if sites.len() >= 2 && !actions.is_empty() {
        let edits: Vec<TextEdit> = sites.iter().map(|s| {
            TextEdit {
                range: Range::from_byte_offsets(&rope, s.start, s.end),
                new_text: s.hash.to_string(),
            }
        }).collect();
        let mut changes = HashMap::new();
        changes.insert(uri.clone(), edits);
        actions.push(CodeAction {
            title: crate::util::i18n::fold_string_hash_all().to_string(),
            kind: Some(CODE_ACTION_KIND_REFACTOR.into()),
            diagnostics: None,
            edit: Some(WorkspaceEdit { changes: Some(changes) }),
            command: None,
        });
    }

    actions
}

/// Recursively walk the tree and collect foldable sites:
/// 1. `StringHash(expr)` calls where the argument evaluates to a string
/// 2. String expressions passed to integer parameter positions
fn collect_string_hash_sites(
    node: &tree_sitter::Node,
    source: &str,
    constants: &std::collections::HashMap<String, ConstValue>,
    signatures: &HashMap<String, Vec<String>>,
    sites: &mut Vec<StringHashSite>,
) {
    if let Ok(Kind::FunctionCall) = Kind::try_from(node.kind_id()) {
        if let Some(name_node) = node.child_by_field_id(FIELD_NAME) {
            let name = &source[name_node.start_byte()..name_node.end_byte()];

            // Case 1: StringHash(expr) — fold the entire call.
            if name == "StringHash" {
                if let Some(args_node) = node.child_by_field_id(FIELD_ARGS) {
                    let mut arg_text = None;
                    for i in 0..args_node.child_count() {
                        if let Some(child) = args_node.child(i as u32) {
                            if let Ok(Kind::Expr) = Kind::try_from(child.kind_id()) {
                                arg_text = Some(&source[child.start_byte()..child.end_byte()]);
                                break;
                            }
                        }
                    }
                    if let Some(expr) = arg_text {
                        if let Some(ConstValue::Str(s)) = eval_const_expr(expr, constants) {
                            let hash = blizzard_string_hash(&s);
                            sites.push(StringHashSite {
                                start: node.start_byte(),
                                end: node.end_byte(),
                                hash,
                            });
                            return;
                        }
                    }
                }
            }

            // Case 2: func(... string_arg ...) where param expects integer.
            if let Some(param_types) = signatures.get(name) {
                if let Some(args_node) = node.child_by_field_id(FIELD_ARGS) {
                    let mut arg_index = 0usize;
                    for i in 0..args_node.child_count() {
                        if let Some(child) = args_node.child(i as u32) {
                            if let Ok(Kind::Expr) = Kind::try_from(child.kind_id()) {
                                if arg_index < param_types.len()
                                    && param_types[arg_index] == "integer"
                                {
                                    let arg_text = &source[child.start_byte()..child.end_byte()];
                                    if let Some(ConstValue::Str(s)) = eval_const_expr(arg_text, constants) {
                                        let hash = blizzard_string_hash(&s);
                                        sites.push(StringHashSite {
                                            start: child.start_byte(),
                                            end: child.end_byte(),
                                            hash,
                                        });
                                    }
                                }
                                arg_index += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    // Recurse into children.
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            collect_string_hash_sites(&child, source, constants, signatures, sites);
        }
    }
}

// ─── `else if` → `elseif` quick fixes ────────────────────────────────────────

fn compute_else_if_fixes(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();
    let uri = &params.text_document.uri;

    let else_if_diags: Vec<_> = params
        .context
        .diagnostics
        .iter()
        .filter(|d| d.has_code("else-if"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    for diag in &else_if_diags {
        let data = match &diag.data {
            Some(d) => d,
            None => continue,
        };

        let else_start_line = data.get("else_start_line").and_then(|v| v.as_u64());
        let else_start_char = data.get("else_start_char").and_then(|v| v.as_u64());
        let if_start_line = data.get("if_start_line").and_then(|v| v.as_u64());
        let if_start_char = data.get("if_start_char").and_then(|v| v.as_u64());
        let if_end_line = data.get("if_end_line").and_then(|v| v.as_u64());
        let if_end_char = data.get("if_end_char").and_then(|v| v.as_u64());
        let ei_el = data.get("inner_endif_end_line").and_then(|v| v.as_u64());

        // ── Fix 1: replace `else if` → `elseif` ─────────────────────────
        if let (Some(esl), Some(esc), Some(isl), Some(isc), Some(iel), Some(iec)) =
            (else_start_line, else_start_char, if_start_line, if_start_char, if_end_line, if_end_char)
        {
            let mut edits = vec![TextEdit {
                range: Range {
                    start: Position { line: esl as usize, character: esc as usize },
                    end: Position { line: iel as usize, character: iec as usize },
                },
                new_text: "elseif".into(),
            }];

            // Re-indent lines when `else` and `if` are on different lines.
            // The delta is the column difference between the inner `if` and
            // the outer `else` (which sits at the same level as the outer `if`).
            let delta = (isc as usize).saturating_sub(esc as usize);
            if delta > 0 && esl != isl {
                if let (Some(reindent_end), Some(rope_ref)) = (ei_el, ROPE_MAP.get(uri)) {
                    let rope = rope_ref.value();
                    let line_count = rope.line_of_offset(rope.len()) + 1;
                    let reindent_start = (iel as usize) + 1;
                    let reindent_end = reindent_end as usize;

                    for line_num in reindent_start..=reindent_end {
                        if line_num >= line_count { break; }
                        let ls = rope.offset_of_line(line_num);
                        let le = rope.offset_of_line(line_num + 1).min(rope.len());
                        let text = rope.slice_to_cow(ls..le);
                        let trimmed = text.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        let leading_ws: usize = text.bytes()
                            .take_while(|b| *b == b' ' || *b == b'\t')
                            .count();
                        let to_strip = delta.min(leading_ws);
                        if to_strip > 0 {
                            edits.push(TextEdit {
                                range: Range {
                                    start: Position { line: line_num, character: 0 },
                                    end: Position { line: line_num, character: to_strip },
                                },
                                new_text: String::new(),
                            });
                        }
                    }
                }
            }

            let mut changes = HashMap::new();
            changes.insert(uri.clone(), edits);
            actions.push(CodeAction {
                title: crate::util::i18n::fix_else_if_to_elseif().to_string(),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit { changes: Some(changes) }),
                command: None,
            });
        }

        // ── Fix 2: add missing `endif` ───────────────────────────────────
        let insert_char = data.get("insert_endif_char").and_then(|v| v.as_u64());

        if let (Some(esc), Some(esl), Some(isl), Some(isc), Some(iel), Some(ic)) =
            (else_start_char, else_start_line, if_start_line, if_start_char, if_end_line, insert_char)
        {
            let mut edits = Vec::new();
            let indent = " ".repeat(ic as usize);

            // Re-indent the inner block when `else if` is on the same line.
            // The body was indented relative to the outer `if`; after adding
            // the outer `endif` we must push it deeper so it nests under the
            // inner `if` keyword.
            let delta = (isc as usize).saturating_sub(esc as usize);
            if delta > 0 && esl == isl {
                if let (Some(reindent_end), Some(rope_ref)) = (ei_el, ROPE_MAP.get(uri)) {
                    let rope = rope_ref.value();
                    let line_count = rope.line_of_offset(rope.len()) + 1;
                    let indent_add = " ".repeat(delta);
                    let reindent_start = (iel as usize) + 1;
                    let reindent_end = reindent_end as usize;

                    for line_num in reindent_start..=reindent_end {
                        if line_num >= line_count { break; }
                        let ls = rope.offset_of_line(line_num);
                        let le = if line_num + 1 < line_count {
                            rope.offset_of_line(line_num + 1)
                        } else {
                            rope.len()
                        };
                        let text = rope.slice_to_cow(ls..le);
                        if text.trim().is_empty() { continue; }
                        edits.push(TextEdit {
                            range: Range {
                                start: Position { line: line_num, character: 0 },
                                end: Position { line: line_num, character: 0 },
                            },
                            new_text: indent_add.clone(),
                        });
                    }
                }
            }

            // Insert the outer `endif` after the inner `endif`.
            let ei_ec = data.get("inner_endif_end_char").and_then(|v| v.as_u64());
            if let (Some(ei_end_line), Some(ei_end_char)) = (ei_el, ei_ec) {
                edits.push(TextEdit {
                    range: Range {
                        start: Position { line: ei_end_line as usize, character: ei_end_char as usize },
                        end: Position { line: ei_end_line as usize, character: ei_end_char as usize },
                    },
                    new_text: format!("\n{}endif", indent),
                });
            } else {
                // No inner `endif` — use the original insertion point.
                let insert_line = data.get("insert_endif_line").and_then(|v| v.as_u64());
                if let Some(line) = insert_line {
                    edits.push(TextEdit {
                        range: Range {
                            start: Position { line: line as usize, character: 0 },
                            end: Position { line: line as usize, character: 0 },
                        },
                        new_text: format!("{}endif\n", indent),
                    });
                }
            }

            if !edits.is_empty() {
                let mut changes = HashMap::new();
                changes.insert(uri.clone(), edits);
                actions.push(CodeAction {
                    title: crate::util::i18n::fix_add_endif().to_string(),
                    kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit { changes: Some(changes) }),
                    command: None,
                });
            }
        }
    }

    actions
}

/// Quick fix for `array-no-init`: remove the `= value` part from an array
/// declaration (arrays cannot have scalar initializers in JASS).
fn compute_array_no_init_fixes(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();
    let uri = &params.text_document.uri;

    let diags: Vec<_> = params
        .context
        .diagnostics
        .iter()
        .filter(|d| d.has_code("array-no-init"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    let rope = ROPE_MAP.get(uri);

    for diag in &diags {
        let data = match &diag.data {
            Some(d) => d,
            None => continue,
        };
        let start = data.get("array_no_init_remove_start").and_then(|v| v.as_u64());
        let end = data.get("array_no_init_remove_end").and_then(|v| v.as_u64());
        if let (Some(start), Some(end), Some(rope)) = (start, end, &rope) {
            let edit = TextEdit {
                range: Range::from_byte_offsets(rope, start as usize, end as usize),
                new_text: String::new(),
            };
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![edit]);
            actions.push(CodeAction {
                title: crate::util::i18n::array_no_init_fix().to_string(),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit { changes: Some(changes) }),
                command: None,
            });
        }
    }

    actions
}

/// Quick fix for `array-set-no-index`: insert `[]` after the variable name
/// so the user can fill in the index.
fn compute_array_set_no_index_fixes(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();
    let uri = &params.text_document.uri;

    let diags: Vec<_> = params
        .context
        .diagnostics
        .iter()
        .filter(|d| d.has_code("array-set-no-index"))
        .filter(|d| d.data.is_some())
        .cloned()
        .collect();

    let rope = ROPE_MAP.get(uri);

    for diag in &diags {
        let data = match &diag.data {
            Some(d) => d,
            None => continue,
        };
        let insert_pos = data.get("array_set_insert_pos").and_then(|v| v.as_u64());
        if let (Some(pos), Some(rope)) = (insert_pos, &rope) {
            let insert_range = Range::from_byte_offsets(rope, pos as usize, pos as usize);
            let edit = TextEdit {
                range: insert_range,
                new_text: "[]".to_string(),
            };
            let mut changes = HashMap::new();
            changes.insert(uri.clone(), vec![edit]);
            actions.push(CodeAction {
                title: crate::util::i18n::array_set_no_index_fix().to_string(),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(vec![diag.clone()]),
                edit: Some(WorkspaceEdit { changes: Some(changes) }),
                command: None,
            });
        }
    }

    actions
}
