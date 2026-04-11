//! Snapshot endpoint — returns the pre-built game data snapshot.
//!
//! `GET /w3e/snapshot` — returns the entire cached snapshot as JSON.
//!
//! When the `archive` query parameter is provided, the snapshot is rebuilt
//! on-the-fly with `war3map.w3d` / `war3map.w3b` merges from the archive
//! so that custom doodads and destructables are included.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SnapshotParams {
    pub token: String,
    /// Optional map archive path — when provided, merge w3d/w3b into the snapshot.
    pub archive: Option<String>,
}

pub async fn snapshot_handler(
    Query(params): Query<SnapshotParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&TokenParam { token: params.token }).map_err(|(s, m)| (s, m.to_string()))?;

    // If an archive path is provided, build a fresh snapshot with w3d/w3b merges.
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

