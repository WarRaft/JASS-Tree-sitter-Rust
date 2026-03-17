use crate::lsp::cancel::CancelId;
use crate::lsp::code_action::lsp::{
    CodeAction, CodeActionParams, Command, CODE_ACTION_KIND_QUICKFIX,
};
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
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
                "⬇ Download UjAPI"
            } else {
                "⬇ Update UjAPI"
            };

            actions.push(CodeAction {
                title: title.to_string(),
                kind: Some(CODE_ACTION_KIND_QUICKFIX.into()),
                diagnostics: Some(ujapi_diags),
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

    actions
}

