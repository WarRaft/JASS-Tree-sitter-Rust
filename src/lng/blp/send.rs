use crate::lng::blp::response::{BlpMipmapMeta, BlpResponse};
use blp::{Blp, FormatDetector};
use serde_json::to_value;
use std::error::Error;
use url::Url;


async fn _send(uri: &Url) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let path = uri.to_file_path().map_err(|()| "Invalid file URI")?;
    let buf = tokio::fs::read(&path).await?;

    let (blp_hdr, frames) = Blp::parse_header(&buf)?;
    let decoded = blp::blp::decode::open_mipmaps_filtered(&blp_hdr, &frames, &buf, &[])?;

    let mipmaps = frames.iter().zip(decoded.iter())
        .map(|(frame, img)| BlpMipmapMeta::from_frame(frame, img.as_ref()))
        .collect();

    let response = BlpResponse { uri, mipmaps };

    Ok(to_value(response)?)
}
