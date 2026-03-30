use crate::lng::w3e::parse::W3eData;
use crate::lng::w3e::slk::load_terrain_slk;
use crate::lsp::cancel::CancelId;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use serde_json::{json, to_value};
use std::error::Error;
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;
use url::Url;

pub async fn send(
    writer: &Arc<Mutex<Stdout>>,
    call_id: Option<CancelId>,
    uri: &Url,
    archive_path: Option<&str>,
) {
    let result_json = _send(uri, archive_path).await.unwrap_or_else(|e| {
        json!({
            "error": {
                "message": e.to_string(),
                "kind": "w3e_render_failure"
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

async fn _send(
    uri: &Url,
    archive_path: Option<&str>,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    if let Some(ap) = archive_path {
        // Opened from an MPQ archive — extract war3map.w3e
        let ap = ap.to_string();
        let file_name = uri
            .to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "war3map.w3e".into());

        // When the URI points to the archive itself (.w3x / .w3m / .w3n / .mpq),
        // the internal file we want is always "war3map.w3e".
        let file_name = {
            let lower = file_name.to_ascii_lowercase();
            if lower.ends_with(".w3x")
                || lower.ends_with(".w3m")
                || lower.ends_with(".w3n")
                || lower.ends_with(".mpq")
            {
                "war3map.w3e".to_string()
            } else {
                file_name
            }
        };

        let buf = tokio::task::spawn_blocking(move || {
            let archive = storm_rs::MpqArchive::open(&ap)
                .map_err(|e| format!("Cannot open archive: {e}"))?;
            archive
                .read_file(&file_name)
                .map_err(|e| format!("Cannot read {file_name}: {e}"))
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))??;

        let (data, meta) = W3eData::read(&buf)?;
        let mut val = to_value(data)?;
        val["_meta"] = to_value(meta)?;

        // Attach terrain SLK tile metadata (blocking FS/MPQ reads).
        let ap2 = archive_path.map(|s| s.to_string());
        let slk = tokio::task::spawn_blocking(move || {
            load_terrain_slk(ap2.as_deref())
        })
        .await
        .ok()
        .flatten();
        if let Some(slk_data) = slk {
            val["_terrainSlk"] = to_value(slk_data)?;
        }

        Ok(val)
    } else {
        // Standalone file — read from disk.
        let path = uri.to_file_path().map_err(|()| "Invalid file URI")?;
        let buf = tokio::fs::read(&path).await?;
        let (data, meta) = W3eData::read(&buf)?;
        let mut val = to_value(data)?;
        val["_meta"] = to_value(meta)?;

        // Attach terrain SLK tile metadata.
        let slk = tokio::task::spawn_blocking(move || {
            load_terrain_slk(None)
        })
        .await
        .ok()
        .flatten();
        if let Some(slk_data) = slk {
            val["_terrainSlk"] = to_value(slk_data)?;
        }

        Ok(val)
    }
}
