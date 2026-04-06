use crate::lng::mdx::parse::parse;
use crate::lng::mdx::response::MdxResponse;
use crate::lsp::cancel::CancelId;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use serde_json::{json, to_value};
use std::error::Error;
use url::Url;

pub async fn send(call_id: Option<CancelId>, uri: &Url) {
    let result_json = _send(uri).await.unwrap_or_else(|e| {
        json!({
            "error": {
                "message": e.to_string(),
                "kind": "mdx_render_failure"
            }
        })
    });

    let response = ResponseMessage {
        jsonrpc: "2.0".into(),
        id: call_id,
        result: Some(result_json),
        error: None,
    };

    let _ = lsp_send(&response).await;
}

async fn _send(uri: &Url) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let path = uri.to_file_path().map_err(|()| "Invalid file URI")?;

    let buf = tokio::fs::read(&path).await?;
    let file_size = buf.len();

    let model = parse(&buf)?;

    // Textures are loaded on-demand by the webview via the HTTP server
    // endpoint /mdx/texture, using the game-folder cascade lookup.
    let response = MdxResponse::from_model(uri, &model, file_size);

    Ok(to_value(response)?)
}

