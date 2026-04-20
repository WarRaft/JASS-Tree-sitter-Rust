//! Cliff types SLK HTTP handler.
//!
//! `GET /mapEditor/cliffTypes` — cliff types SLK data as JSON.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CliffTypesParams {
    pub token: String,
    /// Optional map archive path.
    pub archive: Option<String>,
}

pub async fn cliff_types_handler(
    Query(params): Query<CliffTypesParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&TokenParam { token: params.token })
        .map_err(|(s, m)| (s, m.to_string()))?;

    let archive = params.archive.clone();
    let json = tokio::task::spawn_blocking(move || {
        let cliff_types_slk = crate::lng::map_editor::slk::load_cliff_types_slk(archive.as_deref(), None);
        serde_json::to_vec(&cliff_types_slk).unwrap_or_default()
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

