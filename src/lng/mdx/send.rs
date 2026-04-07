use crate::lng::mdx::parse::parse;
use crate::lng::mdx::response::MdxResponse;
use serde_json::to_value;
use std::error::Error;
use url::Url;


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

