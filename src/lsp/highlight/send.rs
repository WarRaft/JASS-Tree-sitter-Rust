use crate::lsp::cancel::CancelId;
use crate::lsp::highlight::lsp::{DocumentHighlight, DocumentHighlightParams};
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use crate::util::file_store::FILE_STORE;

/// Handle `textDocument/documentHighlight`.
pub async fn send(
    id: Option<CancelId>,
    params: &DocumentHighlightParams,
) {
    let uri = &params.text_document.uri;
    let position = &params.position;

    let result = compute(uri, position);

    lsp_send(
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
    let snapshot = match FILE_STORE.get(uri) {
        Some(s) => s,
        None => return vec![],
    };
    let ref_map = &snapshot.ref_map;

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
