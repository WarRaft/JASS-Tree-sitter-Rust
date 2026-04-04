//! SLK data endpoints — tile textures.
//!
//! All catalog data (terrain, doodads, units, destructables, westrings) is now
//! served via the single `/w3e/snapshot` endpoint.  Only tile textures remain
//! here because they require per-request tile codes and return heavy image data.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;

// ── Tile textures endpoint ──────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TileTexturesParams {
    #[serde(flatten)]
    pub auth: TokenParam,
    pub archive: Option<String>,
    /// Comma-separated rawcode strings, e.g. `"Ldrt,Ldro,Lgrd"`.
    pub codes: String,
}

pub async fn tile_textures_handler(
    Query(params): Query<TileTexturesParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&params.auth).map_err(|(s, m)| (s, m.to_string()))?;
    let archive = params.archive.clone();
    let codes: Vec<String> = params.codes.split(',').map(|s| s.to_string()).collect();

    let result = tokio::task::spawn_blocking(move || {
        use crate::lng::w3e::slk::load_terrain_slk;
        use crate::lng::w3e::textures::load_tile_textures;
        use crate::util::bin_reader::Rawcode;

        let rawcodes: Vec<Rawcode> = codes.into_iter().map(|s| {
            let bytes = s.as_bytes();
            let mut b = [0u8; 4];
            for (i, &byte) in bytes.iter().take(4).enumerate() {
                b[i] = byte;
            }
            Rawcode::from_bytes(b)
        }).collect();
        let slk = load_terrain_slk(archive.as_deref());
        load_tile_textures(&rawcodes, slk.as_ref(), archive.as_deref())
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?;

    let json = serde_json::to_vec(&result)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("JSON error: {e}")))?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        json,
    ))
}
