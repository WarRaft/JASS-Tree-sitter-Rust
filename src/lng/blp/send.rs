use crate::lng::blp::response::{BlpMipmapMeta, BlpResponse};
use blp::core::image::ImageBlp;
use serde_json::to_value;
use std::error::Error;
use url::Url;


async fn _send(uri: &Url) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let path = uri.to_file_path().map_err(|()| "Invalid file URI")?;

    let buf = tokio::fs::read(&path).await?;

    let mut image = ImageBlp::from_buf(&buf)?;

    image.decode(&buf, &[])?;

    let mipmaps = image.mipmaps.iter().map(BlpMipmapMeta::from).collect();

    let response = BlpResponse { uri, mipmaps };

    Ok(to_value(response)?)
}
