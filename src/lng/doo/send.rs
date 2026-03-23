use crate::lng::doo::parse::DooData;
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
                "kind": "doo_render_failure"
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
    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // war3mapUnits.doo → units, war3map.doo → doodads
    let is_unit = fname.to_ascii_lowercase().contains("units");

    let buf = tokio::fs::read(&path).await?;
    let (data, meta) = DooData::read(&buf, is_unit, 26)?;
    let mut val = to_value(data)?;
    val["_meta"] = to_value(meta)?;
    Ok(val)
}

