//! Water SLK HTTP handler.
//!
//! `GET /mapEditor/water` — water SLK data as JSON.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct WaterParams {
    pub token: String,
    /// Optional map archive path.
    pub archive: Option<String>,
}

pub async fn water_handler(
    Query(params): Query<WaterParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&TokenParam { token: params.token })
        .map_err(|(s, m)| (s, m.to_string()))?;

    let archive = params.archive.clone();
    let json = tokio::task::spawn_blocking(move || {
        // Water SLK needs the tileset letter (set when w3e is parsed)
        let water_slk = crate::lng::map_editor::game_path::get_tileset()
            .and_then(|ts| crate::lng::map_editor::slk::load_water_slk(archive.as_deref(), &ts));
        serde_json::to_vec(&water_slk).unwrap_or_default()
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

