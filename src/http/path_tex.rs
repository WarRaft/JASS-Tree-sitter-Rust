//! Path texture endpoint: `GET /w3e/pathTex?token=...&path=...&archive=...`
//!
//! Loads a TGA pathing texture via cascading file lookup, decodes it,
//! and returns per-pixel RGB data as JSON.
//!
//! In Warcraft III pathing textures:
//! - R channel = walkability (0x00 = walkable, 0xFF = unwalkable)
//! - G channel = flyability  (0x00 = flyable,  0xFF = unflyable)
//! - B channel = buildability(0x00 = buildable,0xFF = unbuildable)

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use image::ImageFormat;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct PathTexParams {
    #[serde(flatten)]
    pub auth: TokenParam,
    /// Game-internal path (e.g. `"PathTextures\\4x4Default.tga"`).
    pub path: String,
    /// Optional archive path.
    pub archive: Option<String>,
}

#[derive(Serialize)]
pub struct PathTexResult {
    pub width: u32,
    pub height: u32,
    /// Flat array of [R, G, B, R, G, B, ...] for each pixel, row by row (top to bottom).
    pub pixels: Vec<u8>,
    pub source: String,
}

pub async fn path_tex_handler(
    Query(params): Query<PathTexParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&params.auth).map_err(|(s, m)| (s, m.to_string()))?;

    let path = params.path.clone();
    let archive = params.archive.clone();

    let result = tokio::task::spawn_blocking(move || {
        decode_path_texture(&path, archive.as_deref())
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?;

    match result {
        Some(data) => {
            let json = serde_json::to_string(&data)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("JSON error: {e}")))?;
            Ok((
                [
                    (header::CONTENT_TYPE, "application/json"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                json,
            ))
        }
        None => Err((StatusCode::NOT_FOUND, "Path texture not found".into())),
    }
}

fn decode_path_texture(path: &str, archive: Option<&str>) -> Option<PathTexResult> {
    let (buf, source) =
        crate::lng::map_editor::file_lookup::lookup_file(path, archive)?;

    let img = image::load_from_memory_with_format(&buf, ImageFormat::Tga)
        .ok()
        .or_else(|| image::load_from_memory(&buf).ok())?;

    let rgba = img.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();

    // Extract only RGB channels (skip alpha)
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for pixel in rgba.pixels() {
        pixels.push(pixel[0]); // R
        pixels.push(pixel[1]); // G
        pixels.push(pixel[2]); // B
    }

    Some(PathTexResult {
        width,
        height,
        pixels,
        source,
    })
}

