//! Game-path endpoints — set/get game installation path.
//!
//! `GET  /w3e/gamePath/status` — return current status.
//! `POST /w3e/gamePath/set`    — update the game path, return new status.

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

    let status = crate::lng::w3e::game_path::build_status();
    let json = serde_json::to_vec(&status)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("JSON error: {e}")))?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
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

    crate::lng::w3e::game_path::set_game_path(&body.game_path);
    let status = crate::lng::w3e::game_path::build_status();
    let json = serde_json::to_vec(&status)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("JSON error: {e}")))?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        json,
    ))
}

