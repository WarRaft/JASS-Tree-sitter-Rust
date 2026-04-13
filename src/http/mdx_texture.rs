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
    /// Optional tileset letter (e.g. `"L"`) — enables lookup in `{tileset}.mpq`.
    pub tileset: Option<String>,
}

pub async fn mdx_texture_handler(
    Query(params): Query<MdxTextureParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&params.auth).map_err(|(s, m)| (s, m.to_string()))?;

    let path = params.path.clone();
    let archive = params.archive.clone();
    let tileset = params.tileset.clone();

    let png_bytes = tokio::task::spawn_blocking(move || {
        render_texture_png(&path, archive.as_deref(), tileset.as_deref())
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?
    .map_err(|e| (StatusCode::NOT_FOUND, e))?;

    Ok((
        [
            (header::CONTENT_TYPE, "image/png".to_string()),
            (header::CACHE_CONTROL, "max-age=300".to_string()),
        ],
        png_bytes,
    ))
}

/// Find a texture via the game-folder cascade, decode BLP → PNG bytes.
fn render_texture_png(relative_path: &str, archive_path: Option<&str>, tileset: Option<&str>) -> Result<Vec<u8>, String> {
    use blp::{Blp, FormatDetector, ImageDecoder};
    use image::{DynamicImage, ImageFormat};
    use std::io::Cursor;

    // If the path has no file extension, add .blp so the cascade
    // triggers the .tga → .blp fallback logic.
    let lower = relative_path.to_ascii_lowercase();
    let last_sep = lower.rfind(['/', '\\']).unwrap_or(0);
    let search_path = if !lower[last_sep..].contains('.') {
        format!("{relative_path}.blp")
    } else {
        relative_path.to_string()
    };

    // lookup_file_resolved_ext handles .tga/.blp fallback internally:
    // strips the extension, tries .tga first, then .blp.
    let (buf, _source, resolved_path) = crate::lng::map_editor::file_lookup::lookup_file_resolved_ext(
        &search_path, archive_path, tileset,
    )
    .ok_or_else(|| format!("Texture not found: {relative_path}"))?;

    // Determine format by the actually resolved path's extension
    let resolved_lower = resolved_path.to_ascii_lowercase();

    let rgba = if resolved_lower.ends_with(".blp") {
        let img = Blp::into_dynamic(&buf)
            .map_err(|e| format!("BLP decode error: {e}"))?;
        img.to_rgba8()
    } else if resolved_lower.ends_with(".tga") {
        let img = image::load_from_memory_with_format(&buf, ImageFormat::Tga)
            .map_err(|e| format!("TGA decode error: {e}"))?;
        img.to_rgba8()
    } else {
        // Try BLP first (most common), fall back to generic image decode
        if Blp::detect(&buf) {
            let img = Blp::into_dynamic(&buf)
                .map_err(|e| format!("BLP decode error: {e}"))?;
            img.to_rgba8()
        } else if let Ok(img) = image::load_from_memory(&buf) {
            img.to_rgba8()
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
