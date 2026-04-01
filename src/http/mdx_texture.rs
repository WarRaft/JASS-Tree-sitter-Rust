//! Texture endpoint for MDX viewer: `GET /mdx/texture?token=...&path=...&archive=...`
//!
//! Looks up a game-relative texture path (e.g. `Textures\Knight.blp`) via the
//! cascading file lookup (map archive → game folder → MPQ chain), decodes
//! BLP → RGBA → PNG, and returns raw PNG bytes.
//!
//! The webview fetches images directly from this endpoint — zero base64/data-URL
//! overhead in the LSP JSON response.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct MdxTextureParams {
    #[serde(flatten)]
    pub auth: TokenParam,
    /// Game-relative texture path (e.g. `"Textures\\Knight.blp"`).
    pub path: String,
    /// Optional archive path (map MPQ) for cascade lookup.
    pub archive: Option<String>,
}

pub async fn mdx_texture_handler(
    Query(params): Query<MdxTextureParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&params.auth).map_err(|(s, m)| (s, m.to_string()))?;

    let path = params.path.clone();
    let archive = params.archive.clone();

    let png_bytes = tokio::task::spawn_blocking(move || {
        render_texture_png(&path, archive.as_deref())
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?
    .map_err(|e| (StatusCode::NOT_FOUND, e))?;

    Ok((
        [
            (header::CONTENT_TYPE, "image/png".to_string()),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".to_string()),
            (header::CACHE_CONTROL, "max-age=300".to_string()),
        ],
        png_bytes,
    ))
}

/// Find a texture via the game-folder cascade, decode BLP → PNG bytes.
fn render_texture_png(relative_path: &str, archive_path: Option<&str>) -> Result<Vec<u8>, String> {
    use blp::core::image::ImageBlp;
    use image::{DynamicImage, ImageFormat};
    use std::io::Cursor;

    let (buf, _source) = crate::lng::w3e::file_lookup::lookup_file(relative_path, archive_path)
        .ok_or_else(|| format!("Texture not found: {relative_path}"))?;

    // Determine format by extension
    let lower = relative_path.to_ascii_lowercase();

    let rgba = if lower.ends_with(".blp") {
        let mut image = ImageBlp::from_buf(&buf)
            .map_err(|e| format!("BLP parse error: {e}"))?;
        image.decode(&buf, &[])
            .map_err(|e| format!("BLP decode error: {e}"))?;
        let mipmap = image.mipmaps.first()
            .ok_or("BLP has no mipmaps")?;
        mipmap.image.clone()
            .ok_or_else(|| "BLP mipmap has no image data".to_string())?
    } else if lower.ends_with(".tga") {
        let img = image::load_from_memory_with_format(&buf, ImageFormat::Tga)
            .map_err(|e| format!("TGA decode error: {e}"))?;
        img.to_rgba8()
    } else {
        // Try BLP first (most common), fall back to generic image decode
        if let Ok(mut image) = ImageBlp::from_buf(&buf) {
            if image.decode(&buf, &[]).is_ok() {
                if let Some(mip) = image.mipmaps.first() {
                    if let Some(rgba) = &mip.image {
                        rgba.clone()
                    } else {
                        return Err("Unknown texture format".into());
                    }
                } else {
                    return Err("BLP has no mipmaps".into());
                }
            } else {
                return Err("Failed to decode texture".into());
            }
        } else {
            return Err(format!("Unsupported texture format: {relative_path}"));
        }
    };

    // Encode RGBA → PNG
    let dynamic = DynamicImage::ImageRgba8(rgba);
    let mut cursor = Cursor::new(Vec::new());
    dynamic.write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| format!("PNG encode error: {e}"))?;

    Ok(cursor.into_inner())
}

