//! Binary terrain data endpoint: `GET /w3e/terrain?token=...&uri=...&archive=...`
//!
//! Returns raw binary — the webview wraps it in TypedArray views with zero
//! copy overhead.
//!
//! ## Binary layout (all little-endian)
//!
//! ```text
//! Offset  Type        Field
//! ─────────────────────────────────────
//! 0       u32         W  (map width)
//! 4       u32         H  (map height)
//! 8       f32         offsetX
//! 12      f32         offsetY
//! 16      u32         totalTiles (ground tile count)
//! 20      Uint16[N]   groundHeight    (N = W * H, 2N bytes)
//! 20+2N   Uint16[N]   waterHeight     (2N bytes)
//! 20+4N   Uint8[N]    groundTexture
//! 20+5N   Uint8[N]    groundVariation
//! 20+6N   Uint8[N]    cliffVariation
//! 20+7N   Uint8[N]    cliffTexture
//! 20+8N   Uint8[N]    layerHeight
//! 20+9N   Uint8[N]    flags (bit0=water, bit1=boundary, bit2=blight, bit3=ramp)
//! ```

use crate::http::server::{TokenParam, check_token};
use crate::lng::w3e::parse::W3eData;
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct TerrainParams {
    #[serde(flatten)]
    pub auth: TokenParam,
    /// File URI (file:///path/to/war3map.w3e or the archive itself).
    pub uri: String,
    /// Optional archive path for MPQ-contained files.
    pub archive: Option<String>,
}

pub async fn terrain_handler(
    Query(params): Query<TerrainParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&params.auth).map_err(|(s, m)| (s, m.to_string()))?;

    let uri: url::Url = params
        .uri
        .parse()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Bad URI: {e}")))?;

    let buf = load_w3e_bytes(&uri, params.archive.as_deref())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let (data, _meta) = W3eData::read(&buf)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Parse error: {e}")))?;

    // Store the tileset globally so all file lookups include {tileset}.mpq
    crate::lng::map_editor::game_path::set_tileset(&data.tileset);

    let binary = pack_terrain_binary(&data);

    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        binary,
    ))
}

/// Load the raw .w3e bytes — either from disk or from an MPQ archive.
async fn load_w3e_bytes(
    uri: &url::Url,
    archive_path: Option<&str>,
) -> Result<Vec<u8>, String> {
    if let Some(ap) = archive_path {
        // From MPQ archive
        let ap = ap.to_string();
        let file_name = uri
            .to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "war3map.w3e".into());

        let file_name = {
            let lower = file_name.to_ascii_lowercase();
            if lower.ends_with(".w3x")
                || lower.ends_with(".w3m")
                || lower.ends_with(".w3n")
                || lower.ends_with(".mpq")
            {
                "war3map.w3e".to_string()
            } else {
                file_name
            }
        };

        tokio::task::spawn_blocking(move || {
            let archive = storm_rs::MpqArchive::open(&ap)
                .map_err(|e| format!("Cannot open archive: {e}"))?;
            archive
                .read_file(&file_name)
                .map_err(|e| format!("Cannot read {file_name}: {e}"))
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    } else {
        // From disk
        let path = uri.to_file_path().map_err(|()| "Invalid file URI".to_string())?;
        tokio::fs::read(&path)
            .await
            .map_err(|e| format!("Cannot read file: {e}"))
    }
}

/// Pack the parsed W3E data into a flat binary buffer.
/// Layout matches the doc comment at the top of this module.
fn pack_terrain_binary(data: &W3eData) -> Vec<u8> {
    let w = data.map_width as u32;
    let h = data.map_height as u32;
    let n = data.points.len();
    let total_tiles = data.ground_tiles.len() as u32;

    // Header: 5 × 4 = 20 bytes
    // Body:   2N (groundHeight) + 2N (waterHeight) + 6N = 10N bytes
    let capacity = 20 + 10 * n;
    let mut buf = Vec::with_capacity(capacity);

    // Header
    buf.extend_from_slice(&w.to_le_bytes());
    buf.extend_from_slice(&h.to_le_bytes());
    buf.extend_from_slice(&data.offset_x.to_le_bytes());
    buf.extend_from_slice(&data.offset_y.to_le_bytes());
    buf.extend_from_slice(&total_tiles.to_le_bytes());

    // groundHeight: Uint16Array
    for p in &data.points {
        buf.extend_from_slice(&p.ground_height.to_le_bytes());
    }
    // waterHeight: Uint16Array
    for p in &data.points {
        buf.extend_from_slice(&p.water_height.to_le_bytes());
    }
    // groundTexture: Uint8Array
    for p in &data.points {
        buf.push(p.ground_texture);
    }
    // groundVariation: Uint8Array
    for p in &data.points {
        buf.push(p.ground_variation);
    }
    // cliffVariation: Uint8Array
    for p in &data.points {
        buf.push(p.cliff_variation);
    }
    // cliffTexture: Uint8Array
    for p in &data.points {
        buf.push(p.cliff_texture);
    }
    // layerHeight: Uint8Array
    for p in &data.points {
        buf.push(p.layer_height);
    }
    // flags: Uint8Array (bit0=water, bit1=boundary, bit2=blight, bit3=ramp)
    for p in &data.points {
        let mut f: u8 = 0;
        if p.water { f |= 1; }
        // HiveWE uses both map_edge (water_height bit 14) and boundary
        // (textureFlags bit 7) for the boundary display:
        //   if (bottom_left.map_edge || bottom_left.boundary)
        if p.boundary || p.edge_flag { f |= 2; }
        if p.blight { f |= 4; }
        if p.ramp { f |= 8; }
        buf.push(f);
    }

    buf
}

