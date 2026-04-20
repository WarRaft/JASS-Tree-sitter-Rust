//! Decorations HTTP handler.
//!
//! `GET /mapEditor/decorations` — decorations payload as JSON.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;
use std::time::Instant;

#[derive(Deserialize)]
pub struct DecorationsParams {
    pub token: String,
    pub archive: String,
}

pub async fn decorations_handler(
    Query(params): Query<DecorationsParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    crate::debug_log!("decorations_handler archive={}", params.archive);

    check_token(&TokenParam {
        token: params.token,
    })
    .map_err(|(s, m)| (s, m.to_string()))?;

    let request_started_at = Instant::now();
    let ap = params.archive.clone();
    crate::debug_log!("http::decorations_handler START archive={}", ap);

    let json = tokio::task::spawn_blocking(move || {
        let blocking_started_at = Instant::now();
        crate::debug_log!(
            "http::decorations_handler blocking START archive={}",
            ap,
        );
        let payload = crate::lng::map_editor::decorations::build_decorations_for_archive(&ap);
        let built_elapsed_ms = blocking_started_at.elapsed().as_millis();
        let serialize_started_at = Instant::now();
        let json = crate::lng::map_editor::decorations::serialize_decorations_json(&payload);
        crate::debug_log!(
            "http::decorations_handler blocking END archive={}, build_ms={}, serialize_ms={}, payload_bytes={}, placed_total={}",
            ap,
            built_elapsed_ms,
            serialize_started_at.elapsed().as_millis(),
            json.len(),
            payload.placed.len(),
        );
        json
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?;

    crate::debug_log!(
        "http::decorations_handler END archive={}, elapsed_ms={}, response_bytes={}",
        params.archive,
        request_started_at.elapsed().as_millis(),
        json.len(),
    );

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        json,
    ))
}

