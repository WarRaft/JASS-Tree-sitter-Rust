//! Game-path endpoints — set/get game installation path.
//!
//! `GET  /mapEditor/gamePath/status` — return current status.
//! `POST /mapEditor/gamePath/set`    — update the game path, return new status.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct GamePathSetPayload {
    #[serde(rename = "gamePath")]
    pub game_path: String,
}

pub async fn game_path_status_handler(
    Query(params): Query<TokenParam>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&params).map_err(|(s, m)| (s, m.to_string()))?;

    // Ensure the snapshot is built if a game path is set but no snapshot exists
    // (e.g. first status check after restart before the background builder finishes).
    tokio::task::spawn_blocking(|| {
        let gp = crate::lng::map_editor::game_path::get_game_path();
        if !gp.is_empty() && crate::lng::map_editor::snapshot::get_snapshot_json().is_none() {
            crate::lng::map_editor::snapshot::build_snapshot(None);
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?;

    let status = crate::lng::map_editor::game_path::build_status();
    let json = serde_json::to_vec(&status)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("JSON error: {e}")))?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        json,
    ))
}

pub async fn game_path_set_handler(
    Query(params): Query<TokenParam>,
    Json(body): Json<GamePathSetPayload>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&params).map_err(|(s, m)| (s, m.to_string()))?;

    let game_path = body.game_path.clone();
    tokio::task::spawn_blocking(move || {
        crate::lng::map_editor::game_path::set_game_path(&game_path);
        // Eagerly build the full data snapshot so /mapEditor/snapshot is ready.
        crate::lng::map_editor::snapshot::build_snapshot(None);
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?;

    let status = crate::lng::map_editor::game_path::build_status();
    let json = serde_json::to_vec(&status)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("JSON error: {e}")))?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        json,
    ))
}

