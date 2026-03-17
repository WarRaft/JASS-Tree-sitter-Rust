use crate::lsp::cancel::CancelId;
use crate::lsp::code_action::lsp::{
    CodeAction, CodeActionParams, Command, CODE_ACTION_KIND_QUICKFIX,
};
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use crate::util::roper::uri_map::ROPE_MAP;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;

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
    let uri = &params.text_document.uri;
    let mut actions = Vec::new();

    // ── UjAPI download action ────────────────────────────────────────────
    // Look at the line under cursor to see if it's a //import-ujapi! directive
    let rope_entry = match ROPE_MAP.get(uri) {
        Some(e) => e,
        None => return actions,
    };
    let rope = rope_entry.value();

    let line_idx = params.range.start.line;
    let line_count = rope.line_of_offset(rope.len()) + 1;
    if line_idx >= line_count {
        return actions;
    }

    let line_start = rope.offset_of_line(line_idx);
    let line_end = if line_idx + 1 < line_count {
        rope.offset_of_line(line_idx + 1)
    } else {
        rope.len()
    };
    let line_text = rope.slice_to_cow(line_start..line_end);
    let trimmed = line_text.trim();

    if let Some(rest) = trimmed.strip_prefix("//import-ujapi!") {
        let path = rest.trim().to_string();
        if !path.is_empty() {
            let ujapi_diags: Vec<_> = params.context.diagnostics.iter()
                .filter(|d| d.source.as_deref() == Some("ujapi"))
                .cloned()
                .collect();

            // Always offer the action on this line (download / re-download / update).
            let title = if ujapi_diags.iter().any(|d| d.message.contains("not found")) {
                "⬇ Download UjAPI common.j"
            } else if !ujapi_diags.is_empty() {
                "⬇ Re-download UjAPI common.j"
            } else {
                "⬇ Re-download UjAPI common.j"
            };

            actions.push(CodeAction {
                title: title.to_string(),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: if !ujapi_diags.is_empty() {
                    Some(ujapi_diags)
                } else {
                    None
                },
                command: Some(Command {
                    title: title.to_string(),
                    command: "ujapi.download".into(),
                    arguments: Some(vec![
                        json!(uri.to_string()),
                        json!(path),
                    ]),
                }),
            });
        }
    }

    actions
}

