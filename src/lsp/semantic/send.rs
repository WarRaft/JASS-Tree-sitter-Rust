use crate::lsp::cancel::CancelId;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::range::Range;
use crate::lsp::semantic::lsp::SemanticTokens;
use crate::lsp::send::send as lsp_send;
use crate::util::file_store::FILE_STORE;
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;
use url::Url;

pub async fn send(
    writer: &Arc<Mutex<Stdout>>,
    call_id: Option<CancelId>,
    uri: &Url,
    range: Option<Range>,
) {
    let data = FILE_STORE
        .get(uri)
        .map(|snap| snap.value().semantic.read().unwrap().data(range))
        .unwrap_or_default();

    let _ = lsp_send(
        writer,
        &ResponseMessage {
            jsonrpc: "2.0".into(),
            id: call_id,
            result: Some(SemanticTokens { data }),
            error: None,
        },
    )
    .await;
}
