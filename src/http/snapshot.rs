//! Snapshot / decorations / units HTTP handlers.
//!
//! `GET /mapEditor/snapshot` — cached game snapshot as JSON.
//! `GET /mapEditor/decorations` — decorations payload as JSON.
//! `GET /mapEditor/units` — units payload as JSON.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SnapshotParams {
    pub token: String,
    /// Optional map archive path — build archive-context snapshot.
    pub archive: Option<String>,
}

#[derive(Deserialize)]
pub struct DecorationsParams {
    pub token: String,
    pub archive: String,
}

#[derive(Deserialize)]
pub struct UnitsParams {
    pub token: String,
    pub archive: String,
}

pub async fn snapshot_handler(
    Query(params): Query<SnapshotParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&TokenParam { token: params.token }).map_err(|(s, m)| (s, m.to_string()))?;

    // If an archive path is provided, build a fresh archive-context snapshot.
    if let Some(ref archive_path) = params.archive {
        let ap = archive_path.clone();
        let json = tokio::task::spawn_blocking(move || {
            crate::lng::map_editor::snapshot::build_snapshot_for_archive(&ap)
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?;

        return Ok((
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            json,
        ));
    }

    let json = crate::lng::map_editor::snapshot::get_snapshot_json()
        .ok_or_else(|| {
            (StatusCode::SERVICE_UNAVAILABLE, "Snapshot not built yet — set a game path first".to_string())
        })?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        json,
    ))
}

pub async fn decorations_handler(
    Query(params): Query<DecorationsParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&TokenParam { token: params.token }).map_err(|(s, m)| (s, m.to_string()))?;

    let ap = params.archive.clone();
    let json = tokio::task::spawn_blocking(move || {
        let payload = crate::lng::map_editor::decorations::build_decorations_for_archive(&ap);
        crate::lng::map_editor::decorations::serialize_decorations_json(&payload)
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


pub async fn units_handler(
    Query(params): Query<UnitsParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&TokenParam { token: params.token }).map_err(|(s, m)| (s, m.to_string()))?;

    let ap = params.archive.clone();
    let json = tokio::task::spawn_blocking(move || {
        let payload = crate::lng::map_editor::units::build_units_for_archive(&ap);
        crate::lng::map_editor::units::serialize_units_json(&payload)
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
