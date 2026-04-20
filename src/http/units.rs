//! Units HTTP handler.
//!
//! `GET /mapEditor/units` — units payload as JSON.

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct UnitsParams {
    pub token: String,
    pub archive: String,
}

pub async fn units_handler(
    Query(params): Query<UnitsParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    crate::debug_log!("units_handler archive={}", params.archive);

    check_token(&TokenParam {
        token: params.token,
    })
    .map_err(|(s, m)| (s, m.to_string()))?;

    let ap = params.archive.clone();
    let json = tokio::task::spawn_blocking(move || {
        let payload = crate::lng::map_editor::units::build_units_for_archive(&ap);
        crate::lng::map_editor::units::serialize_units_json(&payload)
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Task error: {e}"),
        )
    })?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        json,
    ))
}

