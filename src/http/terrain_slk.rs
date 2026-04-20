//! Terrain SLK HTTP handler.
//!
//! `GET /mapEditor/terrainSlk` — terrain SLK data as JSON.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct TerrainSlkParams {
    pub token: String,
    /// Optional map archive path.
    pub archive: Option<String>,
}

pub async fn terrain_slk_handler(
    Query(params): Query<TerrainSlkParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&TokenParam { token: params.token })
        .map_err(|(s, m)| (s, m.to_string()))?;

    let archive = params.archive.clone();
    let json = tokio::task::spawn_blocking(move || {
        let terrain_slk = crate::lng::map_editor::slk::load_terrain_slk(archive.as_deref());
        serde_json::to_vec(&terrain_slk).unwrap_or_default()
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        json,
    ))
}

