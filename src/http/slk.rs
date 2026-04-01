//! SLK data endpoints — JSON responses for terrain/doodads/units SLK data.
//!
//! These replace the LSP `w3e/terrainSlk`, `w3e/doodadsSlk`, `w3e/unitsSlk`
//! requests so that non-LSP data doesn't flow through the language protocol.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SlkParams {
    #[serde(flatten)]
    pub auth: TokenParam,
    /// Optional archive path (for looking up SLK inside the map archive first).
    pub archive: Option<String>,
}

pub async fn terrain_slk_handler(
    Query(params): Query<SlkParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&params.auth).map_err(|(s, m)| (s, m.to_string()))?;
    let archive = params.archive.clone();

    let result = tokio::task::spawn_blocking(move || {
        crate::lng::w3e::slk::load_terrain_slk(archive.as_deref())
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?;

    let json = serde_json::to_vec(&result)
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

pub async fn doodads_slk_handler(
    Query(params): Query<SlkParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&params.auth).map_err(|(s, m)| (s, m.to_string()))?;
    let archive = params.archive.clone();

    let result = tokio::task::spawn_blocking(move || {
        crate::lng::w3e::slk::load_doodads_slk(archive.as_deref())
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?;

    let json = serde_json::to_vec(&result)
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

pub async fn units_slk_handler(
    Query(params): Query<SlkParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&params.auth).map_err(|(s, m)| (s, m.to_string()))?;
    let archive = params.archive.clone();

    let result = tokio::task::spawn_blocking(move || {
        crate::lng::w3e::slk::load_units_slk(archive.as_deref())
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?;

    let json = serde_json::to_vec(&result)
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

