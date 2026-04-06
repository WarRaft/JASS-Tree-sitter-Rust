use crate::lng::w3i::W3iData;
use crate::lsp::cancel::CancelId;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use serde_json::{json, to_value};
use std::error::Error;
use url::Url;

pub async fn send(
    call_id: Option<CancelId>,
    uri: &Url,
    archive_path: Option<&str>,
) {
    let result_json = _send(uri, archive_path).await.unwrap_or_else(|e| {
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

    let _ = lsp_send(&response).await;
}

/// Serialize partial W3iData + meta + optional error into a JSON value.
fn build_w3i_json(
    data: W3iData,
    meta: crate::util::bin_reader::BinReaderMeta,
    parse_error: Option<String>,
    wts_map: &std::collections::HashMap<String, String>,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let mut val = to_value(data)?;
    val["_meta"] = to_value(meta)?;
    if let Some(err) = parse_error {
        val["_error"] = json!(err);
    }
    if !wts_map.is_empty() {
        crate::lng::wts::trigstr_resolve::resolve_trigstr_json(&mut val, wts_map);
    }
    Ok(val)
}

async fn _send(
    uri: &Url,
    archive_path: Option<&str>,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    if let Some(ap) = archive_path {
        // Opened from an MPQ archive — read both W3I and WTS from the archive
        // in a single blocking call (same approach as mpq/send get_info).
        let ap = ap.to_string();
        let file_name = uri
            .to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "war3map.w3i".into());

        // When the URI points to the archive itself (.w3x / .w3m / .w3n / .mpq),
        // the internal file we want is always "war3map.w3i".
        let file_name = {
            let lower = file_name.to_ascii_lowercase();
            if lower.ends_with(".w3x")
                || lower.ends_with(".w3m")
                || lower.ends_with(".w3n")
                || lower.ends_with(".mpq")
            {
                "war3map.w3i".to_string()
            } else {
                file_name
            }
        };

        let (w3i_buf, wts_map) = tokio::task::spawn_blocking(move || {
            let archive = storm_rs::MpqArchive::open(&ap)
                .map_err(|e| format!("Cannot open archive: {e}"))?;

            let w3i_buf = archive
                .read_file(&file_name)
                .map_err(|e| format!("Cannot read {file_name}: {e}"))?;

            let wts_map = archive
                .read_file("war3map.wts")
                .map(|data| crate::lng::wts::trigstr_resolve::parse_wts_strings(&data))
                .unwrap_or_default();

            Ok::<_, String>((w3i_buf, wts_map))
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))??;

        let (data, meta, parse_error) = W3iData::read_partial(&w3i_buf);
        build_w3i_json(data, meta, parse_error, &wts_map)
    } else {
        // Standalone file — read from disk.
        let path = uri.to_file_path().map_err(|()| "Invalid file URI")?;
        let buf = tokio::fs::read(&path).await?;
        let (data, meta, parse_error) = W3iData::read_partial(&buf);

        // Look for war3map.wts next to the .w3i file.
        let wts_path = path.parent().map(|d| d.join("war3map.wts"));
        let wts_map = match wts_path {
            Some(wp) => {
                tokio::fs::read(&wp)
                    .await
                    .ok()
                    .map(|data| crate::lng::wts::trigstr_resolve::parse_wts_strings(&data))
                    .unwrap_or_default()
            }
            None => Default::default(),
        };

        build_w3i_json(data, meta, parse_error, &wts_map)
    }
}

