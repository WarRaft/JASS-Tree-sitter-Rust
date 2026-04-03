//! Snapshot endpoint — returns the pre-built game data snapshot.
//!
//! `GET /w3e/snapshot` — returns the entire cached snapshot as JSON.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;

pub async fn snapshot_handler(
    Query(params): Query<TokenParam>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&params).map_err(|(s, m)| (s, m.to_string()))?;

    let json = crate::lng::w3e::snapshot::get_snapshot_json()
        .ok_or_else(|| {
            (StatusCode::SERVICE_UNAVAILABLE, "Snapshot not built yet — set a game path first".to_string())
        })?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        json,
    ))
}

