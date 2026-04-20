//! Cliff variations HTTP handler.
//!
//! `GET /mapEditor/cliffVariations` — cliff variations data as JSON.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CliffVariationsParams {
    pub token: String,
}

pub async fn cliff_variations_handler(
    Query(params): Query<CliffVariationsParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&TokenParam { token: params.token })
        .map_err(|(s, m)| (s, m.to_string()))?;

    let json = tokio::task::spawn_blocking(move || {
        let cliff_variations = crate::lng::map_editor::slk::load_cliff_variations();
        serde_json::to_vec(&cliff_variations).unwrap_or_default()
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

