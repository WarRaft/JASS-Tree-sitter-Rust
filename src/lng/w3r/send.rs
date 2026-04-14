use crate::lng::w3r::W3rData;
use serde_json::to_value;
use std::error::Error;
use url::Url;


async fn _send(
    uri: &Url,
    archive_path: Option<&str>,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let buf = if let Some(ap) = archive_path {
        let ap = ap.to_string();
        tokio::task::spawn_blocking(move || {
            let archive = storm_rs::MpqArchive::open(&ap)
                .map_err(|e| format!("Cannot open archive: {e}"))?;
            archive
                .read_file("war3map.w3r")
                .map_err(|e| format!("Cannot read war3map.w3r: {e}"))
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))??
    } else {
        let path = uri.to_file_path().map_err(|()| "Invalid file URI")?;
        tokio::fs::read(&path).await?
    };

    let (data, meta) = W3rData::read(&buf)?;
    let mut val = to_value(data)?;
    val["_meta"] = to_value(meta)?;
    Ok(val)
}

