//! HTTP route handlers for the custom binary/JSON protocol.
//!
//! All routes are served by axum. Document-sync (open / change / close)
//! uses `POST /document/update` with a binary TLV body. All other
//! requests use JSON.

use crate::http::server::{TokenParam, check_token};
use axum::extract::{Json, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};
use url::Url;

// ─── Auth helper ─────────────────────────────────────────────────────────────

/// POST routes receive the auth token as a query parameter.
#[derive(Deserialize)]
pub struct AuthQuery {
    pub token: String,
}

impl AuthQuery {
    fn check(&self) -> Result<(), (StatusCode, String)> {
        check_token(&TokenParam { token: self.token.clone() })
            .map_err(|(s, m)| (s, m.to_string()))
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Render endpoints (binary file formats)
// ═══════════════════════════════════════════════════════════════════════════════

// ─── BLP ──────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct UriParam {
    pub uri: Url,
}

pub async fn blp_render(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<UriParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let path = params.uri.to_file_path().map_err(|_| (StatusCode::BAD_REQUEST, "Invalid URI".into()))?;
    let buf = tokio::fs::read(&path).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut image = blp::core::image::ImageBlp::from_buf(&buf).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    image.decode(&buf, &[]).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mipmaps: Vec<crate::lng::blp::response::BlpMipmapMeta> = image.mipmaps.iter().map(crate::lng::blp::response::BlpMipmapMeta::from).collect();
    let response = crate::lng::blp::response::BlpResponse { uri: &params.uri, mipmaps };
    Ok(Json(serde_json::to_value(response).unwrap_or_default()))
}

// ─── MDX ──────────────────────────────────────────────────────────────────────

pub async fn mdx_render(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<UriParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let path = params.uri.to_file_path().map_err(|_| (StatusCode::BAD_REQUEST, "Invalid URI".into()))?;
    let buf = tokio::fs::read(&path).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let file_size = buf.len();
    let model = crate::lng::mdx::parse::parse(&buf).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let response = crate::lng::mdx::response::MdxResponse::from_model(&params.uri, &model, file_size);
    Ok(Json(serde_json::to_value(response).unwrap_or_default()))
}

// ─── DOO ──────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DooRenderParams {
    pub uri: Url,
    #[serde(default)]
    pub is_unit: bool,
    pub archive_path: Option<String>,
}

pub async fn doo_render(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<DooRenderParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = doo_render_impl(&params).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(result))
}

async fn doo_render_impl(params: &DooRenderParams) -> Result<Value, String> {
    use crate::lng::doo::parse::DooData;
    let buf = if let Some(ref ap) = params.archive_path {
        let ap = ap.clone();
        let file_name = if params.is_unit { "war3mapUnits.doo".to_string() } else { "war3map.doo".to_string() };
        tokio::task::spawn_blocking(move || {
            let archive = storm_rs::MpqArchive::open(&ap).map_err(|e| format!("Cannot open archive: {e}"))?;
            archive.read_file(&file_name).map_err(|e| format!("Cannot read {file_name}: {e}"))
        }).await.map_err(|e| format!("spawn: {e}"))??
    } else {
        let path = params.uri.to_file_path().map_err(|_| "Invalid URI".to_string())?;
        tokio::fs::read(&path).await.map_err(|e| e.to_string())?
    };
    let (data, meta) = DooData::read(&buf, params.is_unit, 26).map_err(|e| e.to_string())?;
    let mut val = serde_json::to_value(data).map_err(|e| e.to_string())?;
    val["_meta"] = serde_json::to_value(meta).unwrap_or_default();
    Ok(val)
}

// ─── W3I ──────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct W3iRenderParams {
    pub uri: Url,
    pub archive_path: Option<String>,
}

pub async fn w3i_render(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<W3iRenderParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    // Reuse existing w3i logic — it's complex (WTS resolution, partial parse, etc.)
    let result = w3i_render_impl(&params).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(result))
}

async fn w3i_render_impl(params: &W3iRenderParams) -> Result<Value, String> {
    use crate::lng::w3i::W3iData;
    if let Some(ref ap) = params.archive_path {
        let ap = ap.clone();
        let uri = params.uri.clone();
        let file_name = uri.to_file_path().ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "war3map.w3i".into());
        let file_name = {
            let lower = file_name.to_ascii_lowercase();
            if lower.ends_with(".w3x") || lower.ends_with(".w3m") || lower.ends_with(".w3n") || lower.ends_with(".mpq") {
                "war3map.w3i".to_string()
            } else { file_name }
        };
        let (w3i_buf, wts_map) = tokio::task::spawn_blocking(move || {
            let archive = storm_rs::MpqArchive::open(&ap).map_err(|e| format!("Cannot open archive: {e}"))?;
            let w3i_buf = archive.read_file(&file_name).map_err(|e| format!("Cannot read {file_name}: {e}"))?;
            let wts_map = archive.read_file("war3map.wts")
                .map(|data| crate::lng::wts::trigstr_resolve::parse_wts_strings(&data))
                .unwrap_or_default();
            Ok::<_, String>((w3i_buf, wts_map))
        }).await.map_err(|e| format!("spawn: {e}"))??;
        let (data, meta, parse_error) = W3iData::read_partial(&w3i_buf);
        let mut val = serde_json::to_value(data).map_err(|e| e.to_string())?;
        val["_meta"] = serde_json::to_value(meta).unwrap_or_default();
        if let Some(err) = parse_error { val["_error"] = json!(err); }
        if !wts_map.is_empty() { crate::lng::wts::trigstr_resolve::resolve_trigstr_json(&mut val, &wts_map); }
        Ok(val)
    } else {
        let path = params.uri.to_file_path().map_err(|_| "Invalid URI".to_string())?;
        let buf = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
        let (data, meta, parse_error) = W3iData::read_partial(&buf);
        let wts_path = path.parent().map(|d| d.join("war3map.wts"));
        let wts_map = match wts_path {
            Some(wp) => tokio::fs::read(&wp).await.ok()
                .map(|data| crate::lng::wts::trigstr_resolve::parse_wts_strings(&data))
                .unwrap_or_default(),
            None => Default::default(),
        };
        let mut val = serde_json::to_value(data).map_err(|e| e.to_string())?;
        val["_meta"] = serde_json::to_value(meta).unwrap_or_default();
        if let Some(err) = parse_error { val["_error"] = json!(err); }
        if !wts_map.is_empty() { crate::lng::wts::trigstr_resolve::resolve_trigstr_json(&mut val, &wts_map); }
        Ok(val)
    }
}

// ─── W3E render ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct W3eRenderParams {
    pub uri: Url,
    pub archive_path: Option<String>,
}

pub async fn w3e_render(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<W3eRenderParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    // W3E render is very complex — delegate to existing logic.
    // The existing _send function in lng::w3e::send is async and takes CancelId.
    // We call it without CancelId and capture the result.
    let ap = params.archive_path.as_deref();
    // Use the same compute path as the lng module
    let result = crate::lng::w3e::send::compute(&params.uri, ap).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(result))
}

// ─── W3Obj render ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct W3ObjRenderParams {
    pub uri: Url,
    #[serde(default)]
    pub level_data: bool,
    pub archive_path: Option<String>,
}

pub async fn w3obj_render(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<W3ObjRenderParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = w3obj_render_impl(&params).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(result))
}

async fn w3obj_render_impl(params: &W3ObjRenderParams) -> Result<Value, String> {
    use crate::lng::w3abdhqtu::parse::W3ObjectData;
    let buf = if let Some(ref ap) = params.archive_path {
        let ap = ap.clone();
        let file_name = params.uri.to_file_path().ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "war3map.w3u".into());
        tokio::task::spawn_blocking(move || {
            let archive = storm_rs::MpqArchive::open(&ap).map_err(|e| format!("Cannot open archive: {e}"))?;
            archive.read_file(&file_name).map_err(|e| format!("Cannot read {file_name}: {e}"))
        }).await.map_err(|e| format!("spawn: {e}"))??
    } else {
        let path = params.uri.to_file_path().map_err(|_| "Invalid URI".to_string())?;
        tokio::fs::read(&path).await.map_err(|e| e.to_string())?
    };
    let (data, meta) = W3ObjectData::read(&buf, params.level_data).map_err(|e| e.to_string())?;
    let mut val = serde_json::to_value(data).map_err(|e| e.to_string())?;
    val["_meta"] = serde_json::to_value(meta).unwrap_or_default();
    Ok(val)
}

// ─── SLK render ───────────────────────────────────────────────────────────────

pub async fn slk_render(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<UriParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let rope = crate::util::roper::uri_map::ROPE_MAP.get(&params.uri)
        .ok_or((StatusCode::NOT_FOUND, "No rope for URI".into()))?;
    let tree = crate::util::tree_map::TREE_MAP.get(&params.uri)
        .ok_or((StatusCode::NOT_FOUND, "No tree for URI".into()))?;
    // SLK render logic is in lng::slk::send::_send — it's synchronous.
    // We call it via the existing function pattern.
    drop(rope);
    drop(tree);
    let result = crate::lng::slk::send::compute(&params.uri)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(result))
}

// ─── SLK edit ─────────────────────────────────────────────────────────────────

pub async fn slk_edit(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::protocol::SlkEditParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::lng::slk::edit::apply_cell_edit(&params);
    Ok(Json(serde_json::json!(result)))
}

// ═══════════════════════════════════════════════════════════════════════════════
//  W3E catalog endpoints (terrain SLK, doodads SLK, etc.)
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveOptParam {
    pub archive_path: Option<String>,
}

pub async fn w3e_terrain_slk(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<ArchiveOptParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let ap = params.archive_path.clone();
    let slk = tokio::task::spawn_blocking(move || {
        crate::lng::w3e::slk::load_terrain_slk(ap.as_deref())
    }).await.ok().flatten();
    Ok(Json(match slk {
        Some(data) => serde_json::to_value(data).unwrap_or_default(),
        None => json!(null),
    }))
}

pub async fn w3e_doodads_slk(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<ArchiveOptParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let ap = params.archive_path.clone();
    let slk = tokio::task::spawn_blocking(move || {
        crate::lng::w3e::slk::load_doodads_slk(ap.as_deref())
    }).await.ok().flatten();
    Ok(Json(match slk {
        Some(data) => serde_json::to_value(data).unwrap_or_default(),
        None => json!(null),
    }))
}

pub async fn w3e_units_slk(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<ArchiveOptParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let ap = params.archive_path.clone();
    let slk = tokio::task::spawn_blocking(move || {
        crate::lng::w3e::slk::load_units_slk(ap.as_deref())
    }).await.ok().flatten();
    Ok(Json(match slk {
        Some(data) => serde_json::to_value(data).unwrap_or_default(),
        None => json!(null),
    }))
}

pub async fn w3e_destructables_slk(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<ArchiveOptParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let ap = params.archive_path.clone();
    let slk = tokio::task::spawn_blocking(move || {
        crate::lng::w3e::slk::load_destructables_slk(ap.as_deref())
    }).await.ok().flatten();
    Ok(Json(match slk {
        Some(data) => serde_json::to_value(data).unwrap_or_default(),
        None => json!(null),
    }))
}

// ─── W3E file lookup ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupFileParams {
    pub path: String,
    pub archive_path: Option<String>,
}

pub async fn w3e_lookup_file(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<LookupFileParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let path = params.path.clone();
    let ap = params.archive_path.clone();
    let result = tokio::task::spawn_blocking(move || {
        crate::lng::w3e::file_lookup::lookup_file_resolved(&path, ap.as_deref())
    }).await.ok().flatten();
    let result_val = match result {
        Some((buf, source, resolved_path)) => {
            use base64::Engine;
            json!({
                "content": base64::engine::general_purpose::STANDARD.encode(&buf),
                "source": source,
                "resolvedPath": resolved_path,
            })
        }
        None => json!(null),
    };
    Ok(Json(result_val))
}

// ═══════════════════════════════════════════════════════════════════════════════
//  MPQ endpoints
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MpqArchiveParam {
    pub archive_path: String,
}

pub async fn mpq_info(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<MpqArchiveParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let path = params.archive_path.clone();
    let result = tokio::task::spawn_blocking(move || crate::lng::mpq::send::get_info_pub(&path))
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(result))
}

pub async fn mpq_list(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<MpqArchiveParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let path = params.archive_path.clone();
    let result = tokio::task::spawn_blocking(move || crate::lng::mpq::send::list_files_pub(&path))
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "entries": result })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MpqReadParam {
    pub archive_path: String,
    pub file_path: String,
}

pub async fn mpq_read(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<MpqReadParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let apath = params.archive_path.clone();
    let fpath = params.file_path.clone();
    let result = tokio::task::spawn_blocking(move || crate::lng::mpq::send::read_file_pub(&apath, &fpath))
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    use base64::Engine;
    Ok(Json(json!({ "content": base64::engine::general_purpose::STANDARD.encode(&result), "size": result.len() })))
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Graph endpoints
// ═══════════════════════════════════════════════════════════════════════════════

pub async fn graph_import(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<UriParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let (nodes, edges) = crate::util::import_graph::IMPORT_GRAPH.subgraph_for(&params.uri);
    Ok(Json(json!({ "uri": params.uri.to_string(), "nodes": nodes, "edges": edges })))
}

pub async fn graph_call(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<UriParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::util::call_graph::build_call_graph(&params.uri);
    Ok(Json(json!(result)))
}

pub async fn graph_type(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<UriParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::util::type_graph::build_type_graph(&params.uri);
    Ok(Json(json!(result)))
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Build endpoints
// ═══════════════════════════════════════════════════════════════════════════════

pub async fn build_execute(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<UriParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let uri = &params.uri;
    let has_jass = crate::lng::jass::build::has_build_setting(uri, "build-jass");
    let has_as = crate::lng::jass::build::has_build_setting(uri, "build-as");
    let result = if has_jass && has_as {
        let r1 = crate::lng::jass::build::build_jass(uri);
        let r2 = crate::lng::jass::build::build_as(uri);
        if r1.ok && r2.ok {
            crate::lng::jass::build::BuildResult {
                ok: true,
                path: format!("{}, {}", r1.path, r2.path),
                message: format!("JASS: {} | AS: {}", r1.message, r2.message),
            }
        } else if !r1.ok { r1 } else { r2 }
    } else if has_as {
        crate::lng::jass::build::build_as(uri)
    } else {
        crate::lng::jass::build::build_jass(uri)
    };
    Ok(Json(json!(result)))
}

pub async fn build_hooks(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<UriParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let (before_cmd, after_cmd, cwd) = crate::lng::jass::build::resolve_hooks(&params.uri);
    Ok(Json(json!({ "before_cmd": before_cmd, "after_cmd": after_cmd, "cwd": cwd })))
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Rescan endpoint
// ═══════════════════════════════════════════════════════════════════════════════

pub async fn rescan_execute(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<UriParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let uri = &params.uri;
    use crate::util::import_graph::IMPORT_GRAPH;
    use crate::util::file_store::FILE_STORE;

    let tree_uris = IMPORT_GRAPH.tree_for_uri(uri);
    if tree_uris.is_empty() {
        return Ok(Json(json!({ "ok": false, "message": "No files in tree" })));
    }

    let _total = tree_uris.len();
    let tree_list: Vec<Url> = tree_uris.iter().cloned().collect();

    // Purge caches
    crate::util::file_cache::purge_set(&tree_uris);
    crate::util::scope_resolver::SCOPE_RESOLVER.remove_files(&tree_uris);
    for u in &tree_list { FILE_STORE.remove(u); }

    let mut ok_count = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for u in &tree_list {
        let fname = u.path().rsplit('/').next().unwrap_or("");
        match u.to_file_path() {
            Ok(path) if path.is_dir() => continue,
            Ok(path) => match std::fs::read_to_string(&path) {
                Ok(content) => {
                    if let Err(e) = crate::util::open::open_by_uri(u, &content).await {
                        errors.push(format!("{}: {}", fname, e));
                    } else {
                        ok_count += 1;
                    }
                }
                Err(e) => errors.push(format!("{}: cannot read — {}", fname, e)),
            },
            Err(_) => errors.push(format!("{}: invalid file path", fname)),
        }
    }

    crate::util::file_store::send_refresh_all().await;

    let msg = if errors.is_empty() {
        format!("Rescanned {} files", ok_count)
    } else {
        format!("Rescanned {} files ({} errors)\n{}", ok_count, errors.len(), errors.join("\n"))
    };

    Ok(Json(json!({ "ok": errors.is_empty(), "message": msg, "errors": errors })))
}

// ═══════════════════════════════════════════════════════════════════════════════
//  UJAPI download endpoint
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct UjapiDownloadParams {
    pub uri: Url,
    pub path: String,
}

pub async fn ujapi_download(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<UjapiDownloadParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let source_uri = params.uri.clone();
    let path = params.path.clone();
    let uri_for_blocking = params.uri.clone();
    let result = tokio::task::spawn_blocking(move || {
        let dest = match crate::util::ujapi::resolve_ujapi_path(&uri_for_blocking, &path) {
            Some(p) => p,
            None => return json!({ "ok": false, "message": crate::util::i18n::ujapi_cannot_resolve_download_path(&path) }),
        };
        match crate::util::ujapi::download_common_j(&dest) {
            Ok(rel) => json!({
                "ok": true,
                "message": crate::util::i18n::ujapi_downloaded(&rel.tag, &dest.display().to_string()),
                "tag": rel.tag,
                "path": dest.display().to_string()
            }),
            Err(e) => json!({ "ok": false, "message": crate::util::i18n::ujapi_download_failed(&e.to_string()) }),
        }
    }).await.unwrap_or_else(|e| json!({ "ok": false, "message": format!("Task error: {}", e) }));

    // After successful download, re-parse
    if result.get("ok").and_then(|v| v.as_bool()) == Some(true) {
        if let Some(path_str) = result.get("path").and_then(|v| v.as_str()) {
            let dest_path = std::path::PathBuf::from(path_str);
            if let Ok(content) = std::fs::read_to_string(&dest_path) {
                if let Ok(dest_uri) = Url::from_file_path(&dest_path) {
                    let _ = crate::util::open::open_by_uri(&dest_uri, &content).await;
                }
            }
        }
        if let Ok(content) = source_uri.to_file_path().and_then(|p| std::fs::read_to_string(&p).map_err(|_| ())) {
            let _ = crate::util::open::open_by_uri(&source_uri, &content).await;
        }
        crate::util::file_store::send_refresh_all().await;
    }

    Ok(Json(result))
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Language feature endpoints
// ═══════════════════════════════════════════════════════════════════════════════

// ─── Completion ───────────────────────────────────────────────────────────────

pub async fn completion(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::completion::lsp::CompletionParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let items = crate::lsp::completion::send::compute(&params.text_document.uri, &params.position);
    let list = crate::lsp::completion::lsp::CompletionList {
        is_incomplete: items.iter().any(|i| i.kind == Some(crate::lsp::completion::lsp::CompletionItemKind::Folder)),
        items,
    };
    Ok(Json(serde_json::to_value(list).unwrap_or_default()))
}

// ─── Hover ────────────────────────────────────────────────────────────────────

pub async fn hover(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::hover::lsp::HoverParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::lsp::hover::send::compute(&params.text_document.uri, &params.position);
    Ok(Json(match result {
        Some(h) => serde_json::to_value(h).unwrap_or(Value::Null),
        None => Value::Null,
    }))
}

// ─── Document Highlight ───────────────────────────────────────────────────────

pub async fn document_highlight(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::highlight::lsp::DocumentHighlightParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::lsp::highlight::send::compute(&params.text_document.uri, &params.position);
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

// ─── Definition ───────────────────────────────────────────────────────────────

pub async fn definition(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::highlight::lsp::DefinitionParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let uri = &params.text_document.uri;
    let mut locs = Vec::new();
    if let Some(snapshot) = crate::util::file_store::FILE_STORE.get(uri) {
        if let Some(rope_entry) = crate::util::roper::uri_map::ROPE_MAP.get(uri) {
            if let Some(byte) = params.position.to_byte_offset(rope_entry.value()) {
                let ref_map = &snapshot.ref_map;
                if let Some(ext) = ref_map.external_at(byte) {
                    for origin in &ext.origins {
                        if let Some(ext_snap) = crate::util::file_store::FILE_STORE.get(&origin.uri) {
                            for group in ext_snap.ref_map.groups.values() {
                                if group.name == ext.name {
                                    for occ in &group.occurrences {
                                        if occ.is_decl {
                                            locs.push(crate::lsp::location::Location {
                                                uri: origin.uri.to_string(),
                                                range: occ.range.clone(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    for def in ref_map.definitions_at(byte) {
                        locs.push(crate::lsp::location::Location {
                            uri: uri.to_string(),
                            range: def.range.clone(),
                        });
                    }
                }
            }
        }
    }
    Ok(Json(serde_json::to_value(&locs).unwrap_or_default()))
}

// ─── References ───────────────────────────────────────────────────────────────

pub async fn references(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::highlight::lsp::ReferenceParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let uri = &params.text_document.uri;
    let mut locs = Vec::new();
    if let Some(snapshot) = crate::util::file_store::FILE_STORE.get(uri) {
        if let Some(rope_entry) = crate::util::roper::uri_map::ROPE_MAP.get(uri) {
            if let Some(byte) = params.position.to_byte_offset(rope_entry.value()) {
                let include_decl = params.context.include_declaration;
                for occ in snapshot.ref_map.occurrences_at(byte) {
                    if !include_decl && occ.is_decl { continue; }
                    locs.push(crate::lsp::location::Location {
                        uri: uri.to_string(),
                        range: occ.range.clone(),
                    });
                }
            }
        }
    }
    Ok(Json(serde_json::to_value(&locs).unwrap_or_default()))
}

// ─── Formatting ───────────────────────────────────────────────────────────────

pub async fn formatting(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::formatting::lsp::DocumentFormattingParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let uri = &params.text_document.uri;
    let edits: Vec<crate::lsp::formatting::lsp::TextEdit> = if let Some(lng) = crate::util::uri_map::LNG_URI_MAP.get(uri) {
        match lng.value().as_str() {
            "jass" => crate::lsp::formatting::jass::format(uri, &params.options),
            "angelscript" => crate::lsp::formatting::ass::format(uri, &params.options),
            _ => vec![],
        }
    } else { vec![] };

    // Adjust semantic token positions (same as the old send_formatting)
    if !edits.is_empty() {
        let mut deltas: std::collections::HashMap<usize, isize> = std::collections::HashMap::new();
        for edit in &edits {
            if edit.range.start.character == 0 {
                let line = edit.range.start.line;
                let old_len = edit.range.end.character as isize;
                let new_len = edit.new_text.encode_utf16().count() as isize;
                let delta = new_len - old_len;
                if delta != 0 { *deltas.entry(line).or_insert(0) += delta; }
            }
        }
        if !deltas.is_empty() {
            if let Some(snap) = crate::util::file_store::FILE_STORE.get(uri) {
                if let Ok(mut hub) = snap.value().semantic.write() {
                    hub.adjust_columns(&deltas);
                }
            }
        }
    }
    Ok(Json(serde_json::to_value(&edits).unwrap_or_default()))
}

// ─── Prepare Rename ───────────────────────────────────────────────────────────

pub async fn prepare_rename(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::rename::lsp::PrepareRenameParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::lsp::rename::identifier::prepare_rename(&params.text_document.uri, &params.position);
    Ok(Json(match result {
        Some(r) => serde_json::to_value(r).unwrap_or(Value::Null),
        None => Value::Null,
    }))
}

// ─── Rename ───────────────────────────────────────────────────────────────────

pub async fn rename(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::rename::lsp::RenameParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let edit = crate::lsp::rename::identifier::compute_identifier_rename(
        &params.text_document.uri, &params.position, &params.new_name,
    );
    Ok(Json(serde_json::to_value(edit).unwrap_or_default()))
}

// ─── Will Rename Files ────────────────────────────────────────────────────────

pub async fn will_rename_files(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::rename::lsp::RenameFilesParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let edit = crate::lsp::rename::handle::compute_rename_edits(&params.files);
    Ok(Json(serde_json::to_value(edit).unwrap_or_default()))
}

// ─── Color Presentation ──────────────────────────────────────────────────────

pub async fn color_presentation(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::color::lsp::ColorPresentationParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let range_len = if params.range.start.line == params.range.end.line {
        params.range.end.character.saturating_sub(params.range.start.character)
    } else { 0 };
    let is_pipe_color = range_len == 10;
    let presentations = if is_pipe_color {
        let label = crate::lng::string_colors::color_to_pipe_string(&params.color);
        vec![crate::lsp::color::lsp::ColorPresentation {
            label: label.clone(),
            text_edit: Some(crate::lsp::color::lsp::TextEdit { range: params.range.clone(), new_text: label }),
            additional_text_edits: None,
        }]
    } else {
        let label = crate::lng::string_colors::color_to_hex_string(&params.color);
        vec![crate::lsp::color::lsp::ColorPresentation {
            label: label.clone(),
            text_edit: Some(crate::lsp::color::lsp::TextEdit { range: params.range.clone(), new_text: label }),
            additional_text_edits: None,
        }]
    };
    Ok(Json(serde_json::to_value(&presentations).unwrap_or_default()))
}

// ─── Code Action ──────────────────────────────────────────────────────────────

pub async fn code_action(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::code_action::lsp::CodeActionParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let actions = crate::lsp::code_action::send::compute(&params);
    Ok(Json(serde_json::to_value(actions).unwrap_or_default()))
}

// ─── Signature Help ───────────────────────────────────────────────────────────

pub async fn signature_help(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::signature_help::lsp::SignatureHelpParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::lsp::signature_help::send::compute(&params.text_document.uri, &params.position);
    Ok(Json(match result {
        Some(h) => serde_json::to_value(h).unwrap_or(Value::Null),
        None => Value::Null,
    }))
}

// ─── Code Lens ────────────────────────────────────────────────────────────────

pub async fn code_lens(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::code_lens::lsp::CodeLensParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::lsp::code_lens::send::compute(&params.text_document.uri);
    Ok(Json(serde_json::to_value(&result).unwrap_or_default()))
}

// ─── Call Hierarchy ───────────────────────────────────────────────────────────

pub async fn call_hierarchy_prepare(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::call_hierarchy::lsp::CallHierarchyPrepareParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::lsp::call_hierarchy::send::compute_prepare(&params.text_document.uri, &params.position);
    Ok(Json(match result {
        Some(items) => serde_json::to_value(&items).unwrap_or(Value::Null),
        None => Value::Null,
    }))
}

pub async fn call_hierarchy_incoming(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::call_hierarchy::lsp::CallHierarchyIncomingCallsParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::lsp::call_hierarchy::send::compute_incoming(&params.item);
    Ok(Json(serde_json::to_value(&result).unwrap_or_default()))
}

pub async fn call_hierarchy_outgoing(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::call_hierarchy::lsp::CallHierarchyOutgoingCallsParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::lsp::call_hierarchy::send::compute_outgoing(&params.item);
    Ok(Json(serde_json::to_value(&result).unwrap_or_default()))
}

// ─── Type Hierarchy ───────────────────────────────────────────────────────────

pub async fn type_hierarchy_prepare(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::type_hierarchy::lsp::TypeHierarchyPrepareParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::lsp::type_hierarchy::send::compute_prepare(&params.text_document.uri, &params.position);
    Ok(Json(match result {
        Some(items) => serde_json::to_value(&items).unwrap_or(Value::Null),
        None => Value::Null,
    }))
}

pub async fn type_hierarchy_supertypes(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::type_hierarchy::lsp::TypeHierarchySupertypesParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::lsp::type_hierarchy::send::compute_supertypes(&params.item);
    Ok(Json(serde_json::to_value(&result).unwrap_or_default()))
}

pub async fn type_hierarchy_subtypes(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::type_hierarchy::lsp::TypeHierarchySubtypesParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::lsp::type_hierarchy::send::compute_subtypes(&params.item);
    Ok(Json(serde_json::to_value(&result).unwrap_or_default()))
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Document sync — binary TLV protocol
// ═══════════════════════════════════════════════════════════════════════════════

/// Binary TLV section types — shared between request and response.
///
/// Format: `[u8 type][u32 LE byte_count][…data…]` repeated.
/// New section types can be added without breaking existing clients.
mod section {
    // ── Response sections (server → client) ──────────────────────
    /// Semantic tokens (full) — `[u32 resultId][u32 LE token…]`, 5 values per token.
    /// `resultId` is a monotonic ID the client echoes back as `lastResultId`
    /// so the server can compute deltas.
    pub const SEMANTIC: u8 = 0x01;
    /// Inlay hints — packed binary per hint:
    /// `[u32 line][u32 char][u8 kind][u16 label_len][…label UTF-8…]`
    pub const INLAY_HINTS: u8 = 0x02;
    /// Semantic tokens (token-aware delta).
    ///
    /// Payload: `[u32 resultId][...stream of 5×u32 tuples...]`
    ///
    /// Each 5-u32 tuple is either:
    /// - **regular token** `[deltaLine, deltaStartChar, len, type, mods]`
    ///   → append to result
    /// - **COPY** `[0xFFFFFFFF, 0, count, 0, 0]`
    ///   → copy `count` tokens (count×5 u32s) from old array, advance cursor
    /// - **SKIP** `[0xFFFFFFFF, 1, count, 0, 0]`
    ///   → skip `count` tokens in old array (delete), advance cursor
    pub const SEMANTIC_EDIT: u8 = 0x03;

    // ── Request sections (client → server) ───────────────────────
    /// Full document text (open) — raw UTF-8 bytes.
    pub const FULL_TEXT: u8 = 0x10;
    /// Single content change — binary:
    /// `[u32 start_line][u32 start_char][u32 end_line][u32 end_char]
    ///  [u32 text_byte_len][…text UTF-8…]`
    pub const CONTENT_CHANGE: u8 = 0x11;
    /// Open URI — server reads the file from disk itself.
    /// Payload is zero-length (URI is in query params).
    /// Used for `file://` scheme; `mpq://` still uses `FULL_TEXT`.
    pub const OPEN_URI: u8 = 0x12;

    /// Sentinel marker — first u32 of a command tuple in `SEMANTIC_EDIT`.
    pub const SENTINEL: u32 = 0xFFFF_FFFF;
    /// COPY opcode: copy N tokens from old array.
    pub const OP_COPY: u32 = 0;
    /// SKIP opcode: skip (delete) N tokens from old array.
    pub const OP_SKIP: u32 = 1;
}

// ── Per-URI semantic delta state ─────────────────────────────────────────────

/// Monotonic result-ID counter for semantic token responses.
/// Each full or delta response gets a unique ID.
static SEMANTIC_ID_SEQ: AtomicU32 = AtomicU32::new(1);

/// Per-URI last-sent semantic tokens: `(resultId, Vec<u32>)`.
/// Used to compute deltas against what the client last received.
static SEMANTIC_LAST: Lazy<DashMap<Url, (u32, Vec<u32>)>> = Lazy::new(DashMap::new);

/// Query params for `POST /document/update`.
///
/// `version` is a client-side monotonic counter that uniquely identifies the
/// document state the request was built for.  The server echoes it back as the
/// first 4 bytes (`u32 LE`) of the response body so that the client can
/// discard stale responses whose version no longer matches the current
/// document state.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentUpdateQuery {
    pub token: String,
    pub uri: String,
    pub language_id: String,
    /// Client-side document version (monotonic counter).
    #[serde(default)]
    pub version: u32,
    /// Last received semantic-token `resultId`.
    /// If the server's `SEMANTIC_LAST` matches this ID, a token-aware delta
    /// is sent instead of the full token array.  Missing / 0 = send full.
    #[serde(default)]
    pub last_result_id: Option<u32>,
}

/// Build binary TLV response from the current `FILE_STORE` snapshot.
///
/// `prev_result_id` is the client's `lastResultId` — if it matches the
/// server's `SEMANTIC_LAST`, a compact token-aware delta is sent instead of
/// the full token array.
fn build_update_response(uri: &Url, prev_result_id: Option<u32>) -> Vec<u8> {
    use crate::util::file_store::FILE_STORE;

    let mut buf = Vec::new();

    let snap = match FILE_STORE.get(uri) {
        Some(s) => s,
        None => return buf,
    };

    // ── Section 0x01/0x03: Semantic tokens (full or delta) ────────
    let semantic = snap.value().semantic.read().unwrap().data(None);

    // Try to compute a token-aware delta against the previous result.
    let diff: Option<Vec<u32>> = prev_result_id
        .filter(|id| *id != 0)
        .and_then(|client_id| {
            let prev = SEMANTIC_LAST.get(uri)?;
            if prev.0 != client_id { return None; }
            Some(compute_token_diff(&prev.1, &semantic))
            // DashMap guard dropped here
        });

    if let Some(diff_stream) = diff {
        if diff_stream.is_empty() {
            // Tokens unchanged — skip semantic section entirely.
            // Don't update SEMANTIC_LAST — keep the old resultId.
        } else {
            let new_id = SEMANTIC_ID_SEQ.fetch_add(1, Ordering::Relaxed);
            let edit_payload = 4 + diff_stream.len() * 4; // resultId + diff
            let full_payload = 4 + semantic.len() * 4;    // resultId + all tokens

            if edit_payload < full_payload {
                // Send token-aware SEMANTIC_EDIT
                buf.push(section::SEMANTIC_EDIT);
                buf.extend_from_slice(&(edit_payload as u32).to_le_bytes());
                buf.extend_from_slice(&new_id.to_le_bytes());
                for v in &diff_stream {
                    buf.extend_from_slice(&v.to_le_bytes());
                }
            } else {
                // Delta is bigger than full — send full
                encode_semantic_full(&mut buf, new_id, &semantic);
            }
            SEMANTIC_LAST.insert(uri.clone(), (new_id, semantic));
        }
    } else if !semantic.is_empty() {
        // No valid delta base — send full
        let new_id = SEMANTIC_ID_SEQ.fetch_add(1, Ordering::Relaxed);
        encode_semantic_full(&mut buf, new_id, &semantic);
        SEMANTIC_LAST.insert(uri.clone(), (new_id, semantic));
    }

    // ── Section 0x02: Inlay hints ────────────────────────────────
    let hints = snap.all_inlay_hints();
    if !hints.is_empty() {
        let mut section_buf = Vec::new();
        for hint in &hints {
            section_buf.extend_from_slice(&(hint.position.line as u32).to_le_bytes());
            section_buf.extend_from_slice(&(hint.position.character as u32).to_le_bytes());
            section_buf.push(hint.kind as u8);
            let label_bytes = hint.label.as_bytes();
            section_buf.extend_from_slice(&(label_bytes.len() as u16).to_le_bytes());
            section_buf.extend_from_slice(label_bytes);
        }
        buf.push(section::INLAY_HINTS);
        buf.extend_from_slice(&(section_buf.len() as u32).to_le_bytes());
        buf.extend_from_slice(&section_buf);
    }

    buf
}

/// Encode a full `SECTION_SEMANTIC` with resultId prefix.
fn encode_semantic_full(buf: &mut Vec<u8>, result_id: u32, tokens: &[u32]) {
    let payload = 4 + tokens.len() * 4;
    buf.push(section::SEMANTIC);
    buf.extend_from_slice(&(payload as u32).to_le_bytes());
    buf.extend_from_slice(&result_id.to_le_bytes());
    for v in tokens {
        buf.extend_from_slice(&v.to_le_bytes());
    }
}

/// Compute a **token-aware** diff between two delta-encoded semantic token
/// arrays.
///
/// Both arrays must have length divisible by 5 (each token = 5 × u32).
///
/// Returns a diff stream of 5-u32 tuples:
/// - `[0xFFFFFFFF, OP_COPY, count, 0, 0]` — copy `count` tokens from old
/// - `[0xFFFFFFFF, OP_SKIP, count, 0, 0]` — skip `count` tokens in old
/// - `[deltaLine, deltaChar, len, type, mods]` — insert this token
///
/// Returns empty `Vec` if old == new (no change).
fn compute_token_diff(old: &[u32], new: &[u32]) -> Vec<u32> {
    debug_assert!(old.len() % 5 == 0, "old token array not aligned to 5");
    debug_assert!(new.len() % 5 == 0, "new token array not aligned to 5");

    let old_count = old.len() / 5;
    let new_count = new.len() / 5;
    let min_count = old_count.min(new_count);

    // ── Common prefix (in whole tokens) ──────────────────────────
    let mut prefix = 0;
    while prefix < min_count
        && old[prefix * 5..(prefix + 1) * 5] == new[prefix * 5..(prefix + 1) * 5]
    {
        prefix += 1;
    }

    // Arrays are identical
    if prefix == old_count && prefix == new_count {
        return Vec::new();
    }

    // ── Common suffix (in whole tokens, non-overlapping with prefix) ─
    let mut suffix = 0;
    let max_suffix = min_count - prefix;
    while suffix < max_suffix {
        let oi = (old_count - 1 - suffix) * 5;
        let ni = (new_count - 1 - suffix) * 5;
        if old[oi..oi + 5] != new[ni..ni + 5] {
            break;
        }
        suffix += 1;
    }

    let old_mid = old_count - prefix - suffix;
    let new_mid = new_count - prefix - suffix;

    // ── Build diff stream ────────────────────────────────────────
    let mut out = Vec::new();

    // COPY prefix
    if prefix > 0 {
        out.extend_from_slice(&[section::SENTINEL, section::OP_COPY, prefix as u32, 0, 0]);
    }

    // SKIP deleted tokens from old
    if old_mid > 0 {
        out.extend_from_slice(&[section::SENTINEL, section::OP_SKIP, old_mid as u32, 0, 0]);
    }

    // INSERT new tokens
    let ins_start = prefix * 5;
    let ins_end = (new_count - suffix) * 5;
    if ins_start < ins_end {
        out.extend_from_slice(&new[ins_start..ins_end]);
    }

    // COPY suffix
    if suffix > 0 {
        out.extend_from_slice(&[section::SENTINEL, section::OP_COPY, suffix as u32, 0, 0]);
    }

    out
}

/// Parse content changes from binary TLV sections (type 0x11).
fn parse_content_changes(body: &[u8]) -> Result<Vec<crate::lsp::text_document::TextDocumentContentChangeEvent>, String> {
    use crate::lsp::position::Position;
    use crate::lsp::range::Range;

    let mut changes = Vec::new();
    let mut offset = 0usize;

    while offset + 5 <= body.len() {
        let section_type = body[offset]; offset += 1;
        let section_len = u32::from_le_bytes(
            body[offset..offset+4].try_into().map_err(|_| "bad len")?
        ) as usize;
        offset += 4;
        if offset + section_len > body.len() {
            return Err("truncated section".into());
        }
        let data = &body[offset..offset+section_len];
        offset += section_len;

        if section_type != section::CONTENT_CHANGE { continue; }
        if data.len() < 20 {
            return Err("content change too short".into());
        }

        let start_line = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        let start_char = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
        let end_line   = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        let end_char   = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
        let text_len   = u32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;

        if 20 + text_len > data.len() {
            return Err("content change text truncated".into());
        }
        let text = std::str::from_utf8(&data[20..20+text_len])
            .map_err(|e| format!("invalid UTF-8: {e}"))?
            .to_string();

        changes.push(crate::lsp::text_document::TextDocumentContentChangeEvent {
            range: Range {
                start: Position { line: start_line, character: start_char },
                end: Position { line: end_line, character: end_char },
            },
            text,
        });
    }

    Ok(changes)
}

pub async fn document_update(
    Query(params): Query<DocumentUpdateQuery>,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&TokenParam { token: params.token })
        .map_err(|(s, m)| (s, m.to_string()))?;

    let uri: Url = params.uri.parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid URI".into()))?;
    let lang = params.language_id.as_str();
    let version = params.version;

    /// Build a response that only contains the echoed version prefix (no TLV
    /// sections).  Used when there is nothing to return — cancelled parse,
    /// unrecognised language, empty body, etc.
    let empty = |v: u32| Ok((
        [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
        v.to_le_bytes().to_vec(),
    ));

    // ── Cancel any in-flight parse for this URI ──────────────────
    crate::util::file_store::cancel_uri_requests(&uri);
    let cancel = crate::util::file_store::uri_request_token(&uri);

    // ── Parse TLV request body ───────────────────────────────────
    let mut has_work = false;

    if body.len() >= 5 && body[0] == section::OPEN_URI {
        // ── Open by URI — server reads from disk ──────────────────
        let file_path = uri.to_file_path()
            .map_err(|_| (StatusCode::BAD_REQUEST, "URI is not a file:// path".into()))?;
        let text = tokio::fs::read_to_string(&file_path).await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("read {}: {e}", file_path.display())))?;

        let init_result = match lang {
            "bni" => crate::lng::bni::open::init(&uri, &text),
            "jass" => crate::lng::jass::open::init(&uri, &text),
            "angelscript" => crate::lng::ass::open::init(&uri, &text),
            "wts" => crate::lng::wts::open::init(&uri, &text),
            "slk" => crate::lng::slk::open::init(&uri, &text),
            _ => return empty(version),
        };
        if let Err(e) = init_result {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("{lang} init: {e}")));
        }
        has_work = true;
    } else if body.len() >= 5 && body[0] == section::FULL_TEXT {
        // ── Open (full text) ─────────────────────────────────────
        let text_len = u32::from_le_bytes(
            body[1..5].try_into().map_err(|_| (StatusCode::BAD_REQUEST, "bad header".into()))?
        ) as usize;
        if 5 + text_len > body.len() {
            return Err((StatusCode::BAD_REQUEST, "truncated text".into()));
        }
        let text = std::str::from_utf8(&body[5..5+text_len])
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid UTF-8: {e}")))?;

        let init_result = match lang {
            "bni" => crate::lng::bni::open::init(&uri, text),
            "jass" => crate::lng::jass::open::init(&uri, text),
            "angelscript" => crate::lng::ass::open::init(&uri, text),
            "wts" => crate::lng::wts::open::init(&uri, text),
            "slk" => crate::lng::slk::open::init(&uri, text),
            _ => return empty(version),
        };
        if let Err(e) = init_result {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("{lang} init: {e}")));
        }
        has_work = true;
    } else if body.len() >= 5 && body[0] == section::CONTENT_CHANGE {
        // ── Change (incremental edits) ───────────────────────────
        let changes = parse_content_changes(&body)
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

        if !changes.is_empty() {
            let edit_result = match lang {
                "bni" => crate::lng::bni::change::apply_edits(&uri, changes),
                "jass" => crate::lng::jass::change::apply_edits(&uri, changes),
                "angelscript" => crate::lng::ass::change::apply_edits(&uri, changes),
                "wts" => crate::lng::wts::change::apply_edits(&uri, changes),
                "slk" => crate::lng::slk::change::apply_edits(&uri, changes),
                _ => return empty(version),
            };
            if let Err(e) = edit_result {
                return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("{lang} edit: {e}")));
            }
            has_work = true;
        }
    }

    if !has_work {
        return empty(version);
    }

    // ── Parse — race against cancellation ────────────────────────
    let parse_gen = crate::util::file_store::mark_parse_pending(&uri);
    let lang_owned = lang.to_string();
    let uri_clone = uri.clone();
    let mut handle = tokio::spawn(async move {
        let res = match lang_owned.as_str() {
            "bni" => crate::lng::bni::parse::parse_and_notify(&uri_clone).await,
            "jass" => crate::lng::jass::parse::parse_and_notify(&uri_clone, Some(parse_gen)).await,
            "angelscript" => crate::lng::ass::parse::parse_and_notify(&uri_clone, Some(parse_gen)).await,
            "wts" => crate::lng::wts::parse::parse_and_notify(&uri_clone).await,
            "slk" => crate::lng::slk::parse::parse_and_notify(&uri_clone).await,
            _ => Ok(()),
        };
        if let Err(e) = res {
            log::error!("{} parse: {}", lang_owned, e);
        }
        crate::util::file_store::mark_parse_done(&uri_clone, parse_gen);
    });

    // Race: if a newer update cancels us, abort the parse task and return empty.
    tokio::select! {
        _ = &mut handle => {}
        _ = cancel.cancelled() => {
            handle.abort();
            return empty(version);
        }
    }

    // ── Build binary TLV response ────────────────────────────────
    // The first 4 bytes are the echoed client version (u32 LE) so the
    // client can discard stale responses that no longer match its
    // current document state.
    let tlv = build_update_response(&uri, params.last_result_id);
    let mut resp = Vec::with_capacity(4 + tlv.len());
    resp.extend_from_slice(&version.to_le_bytes());
    resp.extend_from_slice(&tlv);
    Ok(([(axum::http::header::CONTENT_TYPE, "application/octet-stream")], resp))
}

pub async fn document_close(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::text_document::DidCloseTextDocumentParams>,
) -> Result<StatusCode, (StatusCode, String)> {
    auth.check()?;
    let uri = params.text_document.uri;
    SEMANTIC_LAST.remove(&uri);
    crate::util::file_store::evict_closed_file(&uri);
    Ok(StatusCode::OK)
}

pub async fn files_changed(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::lsp::text_document::DidChangeWatchedFilesParams>,
) -> Result<StatusCode, (StatusCode, String)> {
    auth.check()?;

    use crate::util::import_graph::IMPORT_GRAPH;
    use crate::util::file_store::FILE_STORE;

    let mut dependents_to_reparse: std::collections::HashSet<Url> =
        std::collections::HashSet::new();

    for event in &params.changes {
        let changed_uri = &event.uri;

        if event.change_type == 3 {
            FILE_STORE.remove(changed_uri);
        }

        if event.change_type == 1 || event.change_type == 2 {
            if IMPORT_GRAPH.all_uris().contains(changed_uri) {
                dependents_to_reparse.insert(changed_uri.clone());
            }
        }

        for dep in IMPORT_GRAPH.direct_dependents(changed_uri) {
            dependents_to_reparse.insert(dep);
        }
    }

    if !dependents_to_reparse.is_empty() {
        tokio::spawn(async move {
            for uri in &dependents_to_reparse {
                if crate::util::roper::uri_map::ROPE_MAP.contains_key(uri) {
                    continue;
                }
                if let Ok(path) = uri.to_file_path() {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Err(e) = crate::util::open::open_by_uri(uri, &content).await {
                            log::error!("file-watcher reparse {}: {}", uri, e);
                        }
                    }
                }
            }
            crate::util::file_store::send_refresh_all().await;
        });
    }

    Ok(StatusCode::OK)
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Semantic tokens — binary protocol
// ═══════════════════════════════════════════════════════════════════════════════

/// Query parameters for `GET /semantic`.
#[derive(Deserialize)]
pub struct SemanticQuery {
    pub token: String,
    pub uri: String,
}

/// Returns the delta-encoded semantic token array as raw little-endian
/// `u32` bytes (`application/octet-stream`).
///
/// The client reads this as a `Uint32Array` — zero JSON overhead.
pub async fn semantic_tokens(
    Query(params): Query<SemanticQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_token(&TokenParam { token: params.token })
        .map_err(|(s, m)| (s, m.to_string()))?;

    let uri: Url = params.uri.parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid URI".into()))?;

    let data = match crate::util::file_store::FILE_STORE.get(&uri) {
        Some(snap) => snap.value().semantic.read().unwrap().data(None),
        None => vec![],
    };

    // Vec<u32> → Vec<u8> little-endian
    let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
        bytes,
    ))
}

