use crate::lng::mdx::parse::parse;
use crate::lng::mdx::response::pack_binary;
use std::error::Error;
use url::Url;


async fn _send(uri: &Url) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    let path = uri.to_file_path().map_err(|()| "Invalid file URI")?;

    let buf = tokio::fs::read(&path).await?;
    let file_size = buf.len();

    let model = parse(&buf)?;

    Ok(pack_binary(&model, file_size))
}
