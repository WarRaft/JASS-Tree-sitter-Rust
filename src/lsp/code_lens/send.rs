use crate::lsp::cancel::CancelId;
use crate::lsp::code_lens::lsp::{CodeLens, Command};
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use crate::util::file_store::FILE_STORE;
use crate::util::uri_map::LNG_URI_MAP;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;
use url::Url;

pub async fn send(
    writer: &Arc<Mutex<Stdout>>,
    id: Option<CancelId>,
    uri: &Url,
) {
    let result = compute(uri);

    lsp_send(
        writer,
        &ResponseMessage::<Value> {
            jsonrpc: "2.0".into(),
            id,
            result: Some(serde_json::to_value(&result).unwrap_or(Value::Null)),
            error: None,
        },
    )
    .await;
}

fn compute(uri: &Url) -> Vec<CodeLens> {
    let lng = match LNG_URI_MAP.get(uri) {
        Some(lng) => lng.value().clone(),
        None => return vec![],
    };
    if lng != "jass" && lng != "angelscript" {
        return vec![];
    }

    let snapshot = match FILE_STORE.get(uri) {
        Some(s) => s,
        None => return vec![],
    };
    let snap = snapshot.value();
    let ref_map = &snap.ref_map;

    let mut lenses = Vec::new();

    for (_key, group) in &ref_map.groups {
        // Find the declaration occurrence
        let decl_occ = match group.occurrences.iter().find(|o| o.is_decl) {
            Some(o) => o,
            None => continue,
        };

        // Count non-declaration references
        let ref_count = group.occurrences.iter().filter(|o| !o.is_decl).count();

        let title = if ref_count == 1 {
            "1 reference".to_string()
        } else {
            format!("{} references", ref_count)
        };

        lenses.push(CodeLens {
            range: decl_occ.range.clone(),
            command: Some(Command {
                title,
                command: "editor.action.showReferences".into(),
                arguments: Some(vec![
                    json!(uri.to_string()),
                    json!({
                        "line": decl_occ.range.start.line,
                        "character": decl_occ.range.start.character,
                    }),
                    json!(group
                        .occurrences
                        .iter()
                        .filter(|o| !o.is_decl)
                        .map(|o| json!({
                            "uri": uri.to_string(),
                            "range": {
                                "start": {
                                    "line": o.range.start.line,
                                    "character": o.range.start.character,
                                },
                                "end": {
                                    "line": o.range.end.line,
                                    "character": o.range.end.character,
                                }
                            }
                        }))
                        .collect::<Vec<_>>()),
                ]),
            }),
            data: None,
        });
    }

    // Sort by line number for consistent display
    lenses.sort_by_key(|l| (l.range.start.line, l.range.start.character));

    lenses
}



