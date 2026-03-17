use crate::lng::w3i::W3iData;
use crate::lsp::cancel::CancelId;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use serde_json::{json, to_value};
use std::error::Error;
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;
use url::Url;

pub async fn send(writer: &Arc<Mutex<Stdout>>, call_id: Option<CancelId>, uri: &Url) {
    let result_json = _send(uri).await.unwrap_or_else(|e| {
        json!({
            "error": {
                "message": e.to_string(),
                "kind": "w3i_render_failure"
            }
        })
    });

    let response = ResponseMessage {
        jsonrpc: "2.0".into(),
        id: call_id,
        result: Some(result_json),
        error: None,
    };

    let _ = lsp_send(writer, &response).await;
}

async fn _send(uri: &Url) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let path = uri.to_file_path().map_err(|()| "Invalid file URI")?;
    let buf = tokio::fs::read(&path).await?;
    let data = W3iData::read(&buf)?;
    Ok(to_value(data)?)
}

