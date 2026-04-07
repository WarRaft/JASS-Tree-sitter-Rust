use crate::lng::doo::parse::DooData;
use serde_json::to_value;
use std::error::Error;
use url::Url;


async fn _send(
    uri: &Url,
    is_unit: bool,
    archive_path: Option<&str>,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    if let Some(ap) = archive_path {
        let ap = ap.to_string();
        let file_name = if is_unit {
            "war3mapUnits.doo".to_string()
        } else {
            "war3map.doo".to_string()
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

        let (data, meta) = DooData::read(&buf, is_unit, 26)?;
        let mut val = to_value(data)?;
        val["_meta"] = to_value(meta)?;
        Ok(val)
    } else {
        let path = uri.to_file_path().map_err(|()| "Invalid file URI")?;
        let buf = tokio::fs::read(&path).await?;
        let (data, meta) = DooData::read(&buf, is_unit, 26)?;
        let mut val = to_value(data)?;
        val["_meta"] = to_value(meta)?;
        Ok(val)
    }
}

