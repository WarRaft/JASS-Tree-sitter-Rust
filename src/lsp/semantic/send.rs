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
    // FILE_STORE is used by both JASS and AS.
    // BNI still uses legacy SEMANTIC_URI_MAP — fall back if needed.
    let data = if let Some(snap) = FILE_STORE.get(uri) {
        snap.value().semantic.data(range)
    } else {
        use crate::lsp::semantic::uri_map::URI_MAP as SEMANTIC_URI_MAP;
        SEMANTIC_URI_MAP
            .get(uri)
            .map(|hub| hub.value().data(range))
            .unwrap_or_default()
    };

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
