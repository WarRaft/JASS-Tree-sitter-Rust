use crate::lng::w3abdhqtu::parse::W3ObjectData;
use serde_json::to_value;
use std::error::Error;
use url::Url;


async fn _send(
    uri: &Url,
    level_data: bool,
    archive_path: Option<&str>,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    if let Some(ap) = archive_path {
        let ap = ap.to_string();
        let file_name = uri
            .to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "war3map.w3u".into());

        let buf = tokio::task::spawn_blocking(move || {
            let archive = storm_rs::MpqArchive::open(&ap)
                .map_err(|e| format!("Cannot open archive: {e}"))?;
            archive
                .read_file(&file_name)
                .map_err(|e| format!("Cannot read {file_name}: {e}"))
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))??;

        let (data, meta) = W3ObjectData::read(&buf, level_data)?;
        let mut val = to_value(data)?;
        val["_meta"] = to_value(meta)?;
        Ok(val)
    } else {
        let path = uri.to_file_path().map_err(|()| "Invalid file URI")?;
        let buf = tokio::fs::read(&path).await?;
        let (data, meta) = W3ObjectData::read(&buf, level_data)?;
        let mut val = to_value(data)?;
        val["_meta"] = to_value(meta)?;
        Ok(val)
    }
}

