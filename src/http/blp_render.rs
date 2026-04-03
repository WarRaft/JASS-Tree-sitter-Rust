//! BLP render endpoint: `GET /blp/render?token=...&path=...&archive=...`
//!
//! Looks up a BLP file via the cascading file resolution (map archive → game
//! folder → game MPQs), decodes it, and returns a JSON array of mipmaps with
//! data-URL encoded PNG images — the same shape as the LSP `blp/render` method.

use crate::http::server::{TokenParam, check_token};
use crate::lng::blp::response::BlpMipmapMeta;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::Json;
use blp::core::image::ImageBlp;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct BlpRenderParams {
    #[serde(flatten)]
    pub auth: TokenParam,
    /// Game-internal path (e.g. `"war3mapMap.blp"`).
    pub path: String,
    /// Optional archive path (for looking up inside the map archive first).
    pub archive: Option<String>,
}

#[derive(Serialize)]
pub struct BlpRenderResponse {
    pub name: String,
    pub mipmaps: Vec<BlpMipmapMeta>,
}

pub async fn blp_render_handler(
    Query(params): Query<BlpRenderParams>,
) -> Result<Json<BlpRenderResponse>, (StatusCode, String)> {
    check_token(&params.auth).map_err(|(s, m)| (s, m.to_string()))?;

    let path = params.path.clone();
    let archive = params.archive.clone();

    let result = tokio::task::spawn_blocking(move || {
        let (buf, _source, resolved) =
            crate::lng::w3e::file_lookup::lookup_file_resolved(&path, archive.as_deref())
                .ok_or_else(|| "File not found".to_string())?;

        let mut image = ImageBlp::from_buf(&buf).map_err(|e| format!("BLP parse error: {e}"))?;
        image.decode(&buf, &[]).map_err(|e| format!("BLP decode error: {e}"))?;

        let mipmaps: Vec<BlpMipmapMeta> = image.mipmaps.iter().map(BlpMipmapMeta::from).collect();

        let name = resolved
            .replace('\\', "/")
            .rsplit('/')
            .next()
            .unwrap_or("image.blp")
            .to_string();

        Ok::<_, String>(BlpRenderResponse { name, mipmaps })
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task error: {e}")))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(result))
}

