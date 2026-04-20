//! Westrings HTTP handler.
//!
//! `GET /mapEditor/westrings` — westrings map as JSON.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct WestringsParams {
    pub token: String,
    /// Optional map archive path.
    pub archive: Option<String>,
}

pub async fn westrings_handler(
    Query(params): Query<WestringsParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&TokenParam { token: params.token })
        .map_err(|(s, m)| (s, m.to_string()))?;

    let archive = params.archive.clone();
    let json = tokio::task::spawn_blocking(move || {
        // Ensure westrings are loaded
        crate::lng::map_editor::westrings::ensure_loaded(archive.as_deref());
        let westrings = crate::lng::map_editor::westrings::get_all();
        serde_json::to_vec(&westrings).unwrap_or_default()
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

