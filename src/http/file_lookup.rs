//! Binary file lookup endpoint: `GET /w3e/file?token=...&path=...&archive=...`
//!
//! Returns raw binary bytes — no base64/JSON overhead.
//! The `X-Source` response header indicates where the file was found
//! (e.g. "map archive", "game folder", "War3.mpq").

use crate::http::server::{TokenParam, check_token};
use axum::extract::Query;
use axum::http::{StatusCode, header, HeaderName};
use axum::response::IntoResponse;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct FileLookupParams {
    #[serde(flatten)]
    pub auth: TokenParam,
    /// Game-internal path (e.g. `"Doodads\\Ashenvale\\Plants\\AshenShrooms\\AshenShrooms.mdx"`).
    pub path: String,
    /// Optional archive path (for looking up inside the map archive first).
    pub archive: Option<String>,
    /// Optional tileset letter (e.g. `"L"`) — enables lookup in `{tileset}.mpq`.
    pub tileset: Option<String>,
}

pub async fn file_lookup_handler(
    Query(params): Query<FileLookupParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&params.auth).map_err(|(s, m)| (s, m.to_string()))?;

    let path = params.path.clone();
    let archive = params.archive.clone();
    let tileset = params.tileset.clone();

    let result = tokio::task::spawn_blocking(move || {
        crate::lng::map_editor::file_lookup::lookup_file_resolved_ext(&path, archive.as_deref(), tileset.as_deref())
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?;

    match result {
        Some((buf, source, resolved_path)) => Ok((
            [
                (header::CONTENT_TYPE, "application/octet-stream".to_string()),
                (header::CACHE_CONTROL, "no-store".to_string()),
                (HeaderName::from_static("x-source"), source),
                (HeaderName::from_static("x-resolved-path"), resolved_path),
            ],
            buf,
        )),
        None => Err((StatusCode::NOT_FOUND, "File not found".into())),
    }
}

