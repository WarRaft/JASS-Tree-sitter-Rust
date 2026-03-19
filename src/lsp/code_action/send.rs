use crate::lsp::cancel::CancelId;
use crate::lsp::code_action::lsp::{
    CodeAction, CodeActionParams, Command, CODE_ACTION_KIND_QUICKFIX, CODE_ACTION_KIND_REFACTOR,
};
use crate::lsp::diagnostic::lsp::Diagnostic;
use crate::lsp::position::Position;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::range::Range;
use crate::lsp::rename::lsp::{TextEdit, WorkspaceEdit};
use crate::lsp::send::send as lsp_send;
use crate::util::file_store::FILE_STORE;
use crate::util::open::is_as_uri;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::tree_map::TREE_MAP;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;
use tree_sitter::Point;

/// Handle `textDocument/codeAction`.
pub async fn send(
    writer: &Arc<Mutex<Stdout>>,
    id: Option<CancelId>,
    params: &CodeActionParams,
) {
    let actions = compute(params);

    lsp_send(
        writer,
        &ResponseMessage::<Value> {
            jsonrpc: "2.0".into(),
            id,
            result: Some(serde_json::to_value(actions).unwrap_or(Value::Null)),
            error: None,
        },
    )
    .await;
}

fn compute(params: &CodeActionParams) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    // ── UjAPI download / re-download actions ──────────────────────────────
    // Diagnostics with source="ujapi" carry { ujapi_uri, ujapi_path } in `data`.
    let ujapi_diags: Vec<_> = params.context.diagnostics.iter()
        .filter(|d| d.source.as_deref() == Some("ujapi"))
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

    actions
}

// ─── AS string format toggle ─────────────────────────────────────────────────

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
        .filter(|d| d.source.as_deref() == Some("leak"))
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
        if let Some(edit) = leak_text_edit(diag, rope) {
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
        .filter(|d| d.source.as_deref() == Some("leak"))
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
        if let Some(edit) = leak_text_edit(diag, rope) {
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
