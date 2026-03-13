use crate::lsp::cancel::CancelId;
use crate::lsp::highlight::lsp::{DocumentHighlight, DocumentHighlightParams};
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::ref_map::REF_URI_MAP;
use crate::lsp::send::send as lsp_send;
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;

/// Handle `textDocument/documentHighlight`.
pub async fn send(
    writer: &Arc<Mutex<Stdout>>,
    id: Option<CancelId>,
    params: &DocumentHighlightParams,
) {
    let uri = &params.text_document.uri;
    let position = &params.position;

    let result = compute(uri, position);

    lsp_send(
        writer,
        &ResponseMessage {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        },
    )
    .await;
}

fn compute(uri: &url::Url, position: &crate::lsp::position::Position) -> Vec<DocumentHighlight> {
    let ref_entry = match REF_URI_MAP.get(uri) {
        Some(e) => e,
        None => return vec![],
    };
    let ref_map = ref_entry.value();

    let rope_entry = match crate::util::roper::uri_map::ROPE_MAP.get(uri) {
        Some(r) => r,
        None => return vec![],
    };
    let byte_offset = match position.to_byte_offset(rope_entry.value()) {
        Some(o) => o,
        None => return vec![],
    };

    ref_map
        .occurrences_at(byte_offset)
        .iter()
        .map(|occ| DocumentHighlight {
            range: occ.range.clone(),
            kind: Some(occ.kind),
        })
        .collect()
}
