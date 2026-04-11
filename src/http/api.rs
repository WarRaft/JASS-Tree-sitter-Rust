//! HTTP route handlers for the custom binary/JSON protocol.
//!
//! All routes are served by axum. Document-sync (open / change / close)
//! uses `POST /document/update` with a binary TLV body. All other
//! requests use JSON.

use crate::debug_log;
use crate::http::server::{TokenParam, check_token};
use axum::extract::{Json, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};
use url::Url;

// ─── Auth helper ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct Location {
    pub uri: String,
    pub range: crate::http::range::Range,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DidCloseTextDocumentParams {
    text_document: DidCloseTextDocumentIdentifier,
}

#[derive(Deserialize)]
struct DidCloseTextDocumentIdentifier {
    uri: Url,
}

#[derive(Deserialize)]
pub(crate) struct DidChangeWatchedFilesParams {
    changes: Vec<FileEvent>,
}

#[derive(Deserialize)]
struct FileEvent {
    uri: Url,
    #[serde(rename = "type")]
    change_type: u8,
}


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

#[derive(Deserialize)]
pub struct ExportsParam {
    pub uri: Url,
    /// "file" | "tree" | "all"  (default = "tree")
    pub mode: Option<String>,
}

#[derive(Deserialize)]
pub struct DiagnosticsParam {
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

/// Params for `slk/edit` — edit a single cell in the SLK table.
#[derive(Debug, Serialize, Deserialize)]
pub struct SlkEditParams {
    pub uri: Url,
    /// Byte offset of the old value in the document.
    pub start: usize,
    /// Byte length of the old value.
    pub len: usize,
    /// New cell value (raw text to insert).
    pub value: String,
}

pub async fn slk_edit(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<SlkEditParams>,
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
        crate::lng::map_editor::slk::load_terrain_slk(ap.as_deref())
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
        crate::lng::map_editor::slk::load_doodads_slk(ap.as_deref())
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
        crate::lng::map_editor::slk::load_units_slk(ap.as_deref())
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
        crate::lng::map_editor::slk::load_destructables_slk(ap.as_deref())
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
        crate::lng::map_editor::file_lookup::lookup_file_resolved(&path, ap.as_deref())
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

pub async fn graph_diagnostics(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<DiagnosticsParam>,
) -> Result<axum::response::Response, (StatusCode, String)> {
    auth.check()?;
    use axum::response::sse::{Event, Sse};
    use crate::util::import_graph::IMPORT_GRAPH;
    use crate::util::parse_cache::PARSE_CACHE;

    let tree_list = IMPORT_GRAPH.tree_for_uri_sorted(&params.uri);
    if tree_list.is_empty() {
        let body = json!({ "done": true, "files": [] });
        return Ok(Json(body).into_response());
    }

    let total = tree_list.len();

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(32);

    tokio::spawn(async move {
        let mut files = Vec::new();

        for (index, peer) in tree_list.iter().enumerate() {
            let file_name = peer.to_file_path()
                .ok()
                .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                .unwrap_or_else(|| peer.to_string());

            let progress = json!({
                "progress": true,
                "index": index,
                "total": total,
                "file": &file_name,
            });
            let _ = tx.send(Ok(Event::default().data(progress.to_string()))).await;

            // Open files → read full diagnostics from PARSE_CACHE
            // Closed files → isolated parse, no side effects
            let (errors, warnings, hints, info_count) = if let Some(snap) = PARSE_CACHE.get(peer) {
                count_snap_diagnostics(&snap)
            } else {
                let peer_clone = peer.clone();
                tokio::task::spawn_blocking(move || count_diagnostics_isolated(&peer_clone))
                    .await
                    .unwrap_or((0, 0, 0, 0))
            };

            let is_frozen = IMPORT_GRAPH.is_frozen(peer);

            let file_entry = json!({
                "uri": peer.to_string(),
                "file": file_name,
                "errors": errors,
                "warnings": warnings,
                "hints": hints,
                "info": info_count,
                "frozen": is_frozen,
            });

            let file_event = json!({
                "file_result": true,
                "entry": &file_entry,
                "index": index,
                "total": total,
            });
            let _ = tx.send(Ok(Event::default().data(file_event.to_string()))).await;

            files.push(file_entry);
        }


        let done = json!({
            "done": true,
            "files": files,
        });
        let _ = tx.send(Ok(Event::default().data(done.to_string()))).await;
    });

    let event_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let sse = Sse::new(event_stream)
        .keep_alive(axum::response::sse::KeepAlive::default());

    Ok(sse.into_response())
}

/// Count diagnostics from an already-parsed snapshot.
fn count_snap_diagnostics(snap: &crate::util::parse_cache::ParseSnapshot) -> (u32, u32, u32, u32) {
    use crate::http::diagnostic::DiagnosticSeverity;
    let (mut e, mut w, mut h, mut i) = (0u32, 0u32, 0u32, 0u32);
    for d in &snap.diagnostics {
        match d.severity {
            Some(DiagnosticSeverity::Error) => e += 1,
            Some(DiagnosticSeverity::Warning) => w += 1,
            Some(DiagnosticSeverity::Hint) => h += 1,
            Some(DiagnosticSeverity::Information) => i += 1,
            None => {}
        }
    }
    (e, w, h, i)
}

/// Isolated diagnostic count for a closed file.
///
/// Reads the file from disk, parses with tree-sitter, runs cursor walk
/// using symbols from PARSE_CACHE/file_cache (read-only), counts diagnostics.
/// Does NOT modify PARSE_CACHE or IMPORT_GRAPH.
fn count_diagnostics_isolated(uri: &Url) -> (u32, u32, u32, u32) {
    use crate::http::diagnostic::DiagnosticSeverity;
    use crate::util::import_graph::IMPORT_GRAPH;
    use crate::util::parse::all_visible_entries;

    let path = match uri.to_file_path() {
        Ok(p) if p.exists() => p,
        _ => return (0, 0, 0, 0),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return (0, 0, 0, 0),
    };
    let rope = lapce_xi_rope::Rope::from(content.as_str());

    let is_as = crate::util::open::is_as_uri(uri);

    // Parse with tree-sitter
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = if is_as {
        tree_sitter_as::language().into()
    } else {
        tree_sitter_jass::language().into()
    };
    if parser.set_language(&lang).is_err() {
        return (0, 0, 0, 0);
    }
    let tree = match parser.parse(&content, None) {
        Some(t) => t,
        None => return (0, 0, 0, 0),
    };

    // Get visible component from existing IMPORT_GRAPH (read-only)
    let component = IMPORT_GRAPH.visible_component(uri);

    // Get imported symbols from PARSE_CACHE/file_cache (read-only)
    let visible_entries = all_visible_entries(&component);

    let diagnostics = if is_as {
        // ── AngelScript ──
        use crate::lng::ass::ast::{build_ast, rewrite_directives};
        use crate::lng::ass::cursor::{Cursor, ImportedSymbol, ImportedKind};
        use crate::util::parse::SymbolNS;
        use crate::http::ref_map::DeclKey;

        let mut ast = build_ast(tree.root_node());
        let src: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
        rewrite_directives(&mut ast, &src);

        let mut imported_symbols: Vec<ImportedSymbol> = Vec::new();
        for entry in &visible_entries {
            if &entry.uri == uri { continue; }
            let is_jass_file = !crate::util::open::is_as_uri(&entry.uri);
            let sym_kind = match entry.ns {
                SymbolNS::Func => ImportedKind::Func,
                SymbolNS::Var => ImportedKind::Var,
            };
            let origin_decl_key = Some(entry.decl_key as DeclKey);

            if is_jass_file {
                imported_symbols.push(ImportedSymbol {
                    origin_uri: entry.uri.clone(),
                    name: entry.name.clone(),
                    kind: sym_kind,
                    origin_decl_key,
                    return_type: entry.return_type.clone(),
                    type_name: entry.type_name.clone(),
                    namespace: "Jass".to_string(),
                });
                imported_symbols.push(ImportedSymbol {
                    origin_uri: entry.uri.clone(),
                    name: entry.name.clone(),
                    kind: sym_kind,
                    origin_decl_key,
                    return_type: entry.return_type.clone(),
                    type_name: entry.type_name.clone(),
                    namespace: String::new(),
                });
            } else {
                imported_symbols.push(ImportedSymbol {
                    origin_uri: entry.uri.clone(),
                    name: entry.name.clone(),
                    kind: sym_kind,
                    origin_decl_key,
                    return_type: entry.return_type.clone(),
                    type_name: entry.type_name.clone(),
                    namespace: entry.namespace.clone(),
                });
            }
        }

        let cursor = Cursor::walk(&ast, &rope, &imported_symbols);
        cursor.diagnostics
    } else {
        // ── JASS ──
        use crate::lng::jass::ast::{build_ast, rewrite_imports};
        use crate::lng::jass::cursor::Cursor;

        let mut ast = build_ast(tree.root_node());
        let src: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
        rewrite_imports(&mut ast, &src);

        let cursor = Cursor::walk(&ast, &rope, &[]);
        cursor.diagnostics
    };

    let (mut e, mut w, mut h, mut i) = (0u32, 0u32, 0u32, 0u32);
    for d in &diagnostics {
        match d.severity {
            Some(DiagnosticSeverity::Error) => e += 1,
            Some(DiagnosticSeverity::Warning) => w += 1,
            Some(DiagnosticSeverity::Hint) => h += 1,
            Some(DiagnosticSeverity::Information) => i += 1,
            None => {}
        }
    }
    (e, w, h, i)
}


pub async fn graph_exports(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<ExportsParam>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    use crate::util::parse::{SymbolNS, all_entries, entries_for_uri};
    use crate::util::import_graph::IMPORT_GRAPH;

    let mode = params.mode.as_deref().unwrap_or("tree");

    /// Collect type names for `uri`.
    ///
    /// Tries `PARSE_CACHE` first (cheapest — in-memory); falls back to the
    /// persistent **disk cache** when the snapshot hasn't been loaded yet.
    fn type_names_for(uri: &Url) -> std::collections::HashSet<String> {
        if let Some(snap) = crate::util::parse_cache::peek_or_load(uri) {
            return snap.file_symbols.types.iter().map(|t| t.name.clone()).collect();
        }
        std::collections::HashSet::new()
    }

    // Helper: emit class/interface/mixin/enum declarations + their members from a file snapshot.
    fn collect_members(uri: &Url, entries: &mut Vec<Value>) {
        let Some(snap) = crate::util::parse_cache::peek_or_load(uri) else { return };
        let fs = &snap.file_symbols;
        let file_name = uri.to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
            .unwrap_or_else(|| uri.to_string());

        // Get the rope for correct byte→Position conversion (handles multi-byte / Cyrillic).
        let rope_ref = crate::util::roper::uri_map::ROPE_MAP.get(uri);
        let rope_opt = rope_ref.as_ref().map(|r| r.value());

        /// Convert a byte offset to `(line, character)` JSON fields using the rope.
        /// Falls back to `(0, 0)` when the rope is unavailable.
        fn decl_pos_fields(rope_opt: Option<&lapce_xi_rope::Rope>, byte: usize) -> (usize, usize) {
            if let Some(rope) = rope_opt {
                if let Some(pos) = crate::http::position::Position::from_byte_offset(rope, byte) {
                    return (pos.line, pos.character);
                }
            }
            (0, 0)
        }

        for c in &fs.classes {
            let (dl, dc) = decl_pos_fields(rope_opt, c.decl_byte);
            // Class declaration itself
            entries.push(json!({
                "name": c.name,
                "ns": "class",
                "class_name": null,
                "namespace": c.namespace,
                "type_name": null,
                "return_type": null,
                "params": "",
                "is_constant": false,
                "is_array": false,
                "uri": uri.to_string(),
                "file": file_name,
                "decl_line": dl,
                "decl_char": dc,
            }));
            for m in &c.methods {
                let (dl, dc) = decl_pos_fields(rope_opt, m.decl_byte);
                entries.push(json!({
                    "name": m.name,
                    "ns": "method",
                    "class_name": c.name,
                    "namespace": c.namespace,
                    "type_name": null,
                    "return_type": m.return_type,
                    "params": m.params.iter().map(|p| format!("{} {}", p.type_name, p.name)).collect::<Vec<_>>().join(", "),
                    "is_constant": false,
                    "is_array": false,
                    "uri": uri.to_string(),
                    "file": file_name,
                    "decl_line": dl,
                    "decl_char": dc,
                }));
            }
            for p in &c.properties {
                let (dl, dc) = decl_pos_fields(rope_opt, p.decl_byte);
                entries.push(json!({
                    "name": p.name,
                    "ns": "property",
                    "class_name": c.name,
                    "namespace": c.namespace,
                    "type_name": p.type_name,
                    "return_type": null,
                    "params": "",
                    "is_constant": false,
                    "is_array": false,
                    "uri": uri.to_string(),
                    "file": file_name,
                    "decl_line": dl,
                    "decl_char": dc,
                }));
            }
        }
        for i in &fs.interfaces {
            let (dl, dc) = decl_pos_fields(rope_opt, i.decl_byte);
            // Interface declaration itself
            entries.push(json!({
                "name": i.name,
                "ns": "interface",
                "class_name": null,
                "namespace": i.namespace,
                "type_name": null,
                "return_type": null,
                "params": "",
                "is_constant": false,
                "is_array": false,
                "uri": uri.to_string(),
                "file": file_name,
                "decl_line": dl,
                "decl_char": dc,
            }));
            for m in &i.methods {
                let (dl, dc) = decl_pos_fields(rope_opt, m.decl_byte);
                entries.push(json!({
                    "name": m.name,
                    "ns": "method",
                    "class_name": i.name,
                    "namespace": i.namespace,
                    "type_name": null,
                    "return_type": m.return_type,
                    "params": m.params.iter().map(|p| format!("{} {}", p.type_name, p.name)).collect::<Vec<_>>().join(", "),
                    "is_constant": false,
                    "is_array": false,
                    "uri": uri.to_string(),
                    "file": file_name,
                    "decl_line": dl,
                    "decl_char": dc,
                }));
            }
        }
        for en in &fs.enums {
            let (dl, dc) = decl_pos_fields(rope_opt, en.decl_byte);
            // Enum declaration itself
            entries.push(json!({
                "name": en.name,
                "ns": "enum",
                "class_name": null,
                "namespace": en.namespace,
                "type_name": null,
                "return_type": null,
                "params": "",
                "is_constant": false,
                "is_array": false,
                "uri": uri.to_string(),
                "file": file_name,
                "decl_line": dl,
                "decl_char": dc,
            }));
        }
        for mx in &fs.mixins {
            let (dl, dc) = decl_pos_fields(rope_opt, mx.decl_byte);
            // Mixin declaration itself
            entries.push(json!({
                "name": mx.name,
                "ns": "mixin",
                "class_name": null,
                "namespace": mx.namespace,
                "type_name": null,
                "return_type": null,
                "params": "",
                "is_constant": false,
                "is_array": false,
                "uri": uri.to_string(),
                "file": file_name,
                "decl_line": dl,
                "decl_char": dc,
            }));
            for m in &mx.methods {
                let (dl, dc) = decl_pos_fields(rope_opt, m.decl_byte);
                entries.push(json!({
                    "name": m.name,
                    "ns": "method",
                    "class_name": mx.name,
                    "namespace": mx.namespace,
                    "type_name": null,
                    "return_type": m.return_type,
                    "params": m.params.iter().map(|p| format!("{} {}", p.type_name, p.name)).collect::<Vec<_>>().join(", "),
                    "is_constant": false,
                    "is_array": false,
                    "uri": uri.to_string(),
                    "file": file_name,
                    "decl_line": dl,
                    "decl_char": dc,
                }));
            }
            for p in &mx.properties {
                let (dl, dc) = decl_pos_fields(rope_opt, p.decl_byte);
                entries.push(json!({
                    "name": p.name,
                    "ns": "property",
                    "class_name": mx.name,
                    "namespace": mx.namespace,
                    "type_name": p.type_name,
                    "return_type": null,
                    "params": "",
                    "is_constant": false,
                    "is_array": false,
                    "uri": uri.to_string(),
                    "file": file_name,
                    "decl_line": dl,
                    "decl_char": dc,
                }));
            }
        }
    }

    let uris_to_scan: Vec<Url> = match mode {
        "file" => vec![params.uri.clone()],
        "all" => {
            // Return all indexed entries
            let all = all_entries();
            let mut entries = Vec::with_capacity(all.len());
            let mut seen_uris = std::collections::HashSet::new();

            // Collect type names per URI so we can label them as "type" instead of "variable".
            let mut type_names_cache: std::collections::HashMap<Url, std::collections::HashSet<String>> = std::collections::HashMap::new();

            for e in &all {
                seen_uris.insert(e.uri.clone());

                // Determine ns label: check if this Var entry is actually a type.
                let ns_str = match e.ns {
                    SymbolNS::Func => "function",
                    SymbolNS::Var => {
                        let is_type = type_names_cache
                            .entry(e.uri.clone())
                            .or_insert_with(|| type_names_for(&e.uri))
                            .contains(&e.name);
                        if is_type { "type" } else { "variable" }
                    }
                };
                let file_name = e.uri.to_file_path()
                    .ok()
                    .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                    .unwrap_or_else(|| e.uri.to_string());
                let (dl, dc) = {
                    let rr = crate::util::roper::uri_map::ROPE_MAP.get(&e.uri);
                    let ro = rr.as_ref().map(|r| r.value());
                    if let Some(rope) = ro {
                        if let Some(pos) = crate::http::position::Position::from_byte_offset(rope, e.decl_key) {
                            (pos.line, pos.character)
                        } else { (0, 0) }
                    } else { (0, 0) }
                };
                entries.push(json!({
                    "name": e.name,
                    "ns": ns_str,
                    "class_name": null,
                    "namespace": e.namespace,
                    "type_name": e.type_name,
                    "return_type": e.return_type,
                    "params": e.params.iter().map(|(n, t)| format!("{} {}", t, n)).collect::<Vec<_>>().join(", "),
                    "is_constant": e.is_constant,
                    "is_array": e.is_array,
                    "uri": e.uri.to_string(),
                    "file": file_name,
                    "decl_line": dl,
                    "decl_char": dc,
                }));
            }
            // Also emit class/interface/mixin/enum declarations + members from snapshots.
            for uri in &seen_uris {
                collect_members(uri, &mut entries);
            }
            return Ok(Json(json!({ "entries": entries })));
        }
        _ => {
            // "tree" (default)
            let tree_uris = IMPORT_GRAPH.tree_for_uri(&params.uri);
            if tree_uris.is_empty() {
                vec![params.uri.clone()]
            } else {
                tree_uris.into_iter().collect()
            }
        }
    };

    let mut entries = Vec::new();
    for uri in &uris_to_scan {
        let rope_ref = crate::util::roper::uri_map::ROPE_MAP.get(uri);
        let rope_opt = rope_ref.as_ref().map(|r| r.value());

        // Collect type names from the snapshot (or disk cache) to label them as "type".
        let type_names = type_names_for(uri);

        for e in entries_for_uri(uri) {
            let ns_str = match e.ns {
                SymbolNS::Func => "function",
                SymbolNS::Var => {
                    if type_names.contains(&e.name) { "type" } else { "variable" }
                }
            };
            let file_name = uri.to_file_path()
                .ok()
                .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                .unwrap_or_else(|| uri.to_string());

            let (dl, dc) = if let Some(rope) = rope_opt {
                if let Some(pos) = crate::http::position::Position::from_byte_offset(rope, e.decl_key) {
                    (pos.line, pos.character)
                } else { (0, 0) }
            } else { (0, 0) };

            entries.push(json!({
                "name": e.name,
                "ns": ns_str,
                "class_name": null,
                "namespace": e.namespace,
                "type_name": e.type_name,
                "return_type": e.return_type,
                "params": e.params.iter().map(|(n, t)| format!("{} {}", t, n)).collect::<Vec<_>>().join(", "),
                "is_constant": e.is_constant,
                "is_array": e.is_array,
                "uri": uri.to_string(),
                "file": file_name,
                "decl_line": dl,
                "decl_char": dc,
            }));
        }
        // Also emit class/interface/mixin/enum declarations + members.
        collect_members(uri, &mut entries);
    }

    Ok(Json(json!({ "entries": entries })))
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
//  Rescan endpoint (SSE with singleton guard)
// ═══════════════════════════════════════════════════════════════════════════════

pub async fn rescan_execute(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<UriParam>,
) -> Result<axum::response::Response, (StatusCode, String)> {
    auth.check()?;
    use axum::response::sse::{Event, Sse};
    use crate::util::import_graph::IMPORT_GRAPH;
    use crate::util::parse_cache::PARSE_CACHE;
    use crate::util::rescan::RescanGuard;

    // Only one rescan at a time — if busy, reply immediately.
    let guard = match RescanGuard::try_acquire() {
        Some(g) => g,
        None => {
            let body = json!({ "busy": true, "message": "Rescan already in progress" });
            return Ok(Json(body).into_response());
        }
    };

    let uri = params.uri.clone();

    let tree_uris = IMPORT_GRAPH.tree_for_uri(&uri);
    if tree_uris.is_empty() {
        drop(guard);
        let body = json!({ "ok": false, "message": "No files in tree" });
        return Ok(Json(body).into_response());
    }

    let total = tree_uris.len();
    let tree_list: Vec<Url> = tree_uris.iter().cloned().collect();

    // Purge caches before the scan loop.
    crate::util::file_cache::purge_set(&tree_uris);
    for u in &tree_list { PARSE_CACHE.remove(u); }

    // Create a channel so the background task can push SSE events.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(32);

    // Spawn the heavy work so the SSE response starts streaming immediately.
    tokio::spawn(async move {
        let _guard = guard; // move guard into spawned task — dropped when done

        // Collect absolute paths for common-parent computation.
        let abs_paths: Vec<std::path::PathBuf> = tree_list
            .iter()
            .filter_map(|u| u.to_file_path().ok())
            .collect();

        // Compute nearest common parent directory.
        let common_parent = if abs_paths.is_empty() {
            std::path::PathBuf::new()
        } else {
            let mut common = abs_paths[0].parent().unwrap_or(&abs_paths[0]).to_path_buf();
            for p in &abs_paths[1..] {
                let dir = p.parent().unwrap_or(p);
                common = common
                    .ancestors()
                    .find(|a| dir.starts_with(a))
                    .unwrap_or(std::path::Path::new("/"))
                    .to_path_buf();
            }
            common
        };

        let mut errors: Vec<String> = Vec::new();
        let mut scanned_files: Vec<String> = Vec::new();
        // File contents read from disk in pass 1, reused in pass 2.
        let mut contents: Vec<Option<String>> = Vec::with_capacity(tree_list.len());

        // ── Pass 1: init (rope + tree) + lightweight symbol collection ──
        for (index, u) in tree_list.iter().enumerate() {
            let fname = u.path().rsplit('/').next().unwrap_or("");

            let progress = json!({
                "step": 1,
                "file": fname,
                "index": index,
                "total": total,
            });
            let _ = tx.send(Ok(Event::default().data(progress.to_string()))).await;

            match u.to_file_path() {
                Ok(path) if path.is_dir() => { contents.push(None); continue; }
                Ok(path) => {
                    let rel = path
                        .strip_prefix(&common_parent)
                        .unwrap_or(&path)
                        .display()
                        .to_string();
                    scanned_files.push(rel);

                    match std::fs::read_to_string(&path) {
                        Ok(content) => {
                            // Init: rope + parser + tree (no diagnostics).
                            if let Err(e) = crate::util::open::init_by_uri(u, &content) {
                                errors.push(format!("{}: init — {}", fname, e));
                                contents.push(None);
                                continue;
                            }
                            // Lightweight symbol extraction → file_cache.
                            let ts_lang: tree_sitter::Language = if crate::util::open::is_as_uri(u) {
                                tree_sitter_as::language().into()
                            } else {
                                tree_sitter_jass::language().into()
                            };
                            crate::util::parse::ensure_file_symbols(u, ts_lang);
                            contents.push(Some(content));
                        }
                        Err(e) => {
                            errors.push(format!("{}: cannot read — {}", fname, e));
                            contents.push(None);
                        }
                    }
                }
                Err(_) => {
                    errors.push(format!("{}: invalid file path", fname));
                    contents.push(None);
                }
            }
        }

        // ── Pass 2: full parse (all symbols now in scope resolver) ──────
        let mut ok_count = 0usize;

        for (index, u) in tree_list.iter().enumerate() {
            let content = match contents.get(index).and_then(|c| c.as_deref()) {
                Some(c) => c,
                None => continue,
            };
            let fname = u.path().rsplit('/').next().unwrap_or("");

            let progress = json!({
                "step": 2,
                "file": fname,
                "index": index,
                "total": total,
            });
            let _ = tx.send(Ok(Event::default().data(progress.to_string()))).await;

            if let Err(e) = crate::util::open::parse_only_by_uri(u, content).await {
                errors.push(format!("{}: parse — {}", fname, e));
            } else {
                ok_count += 1;
            }
        }

        // Collect URI + languageId pairs for client-side refresh.
        let uri_entries: Vec<serde_json::Value> = tree_list.iter().map(|u| {
            let lang = if crate::util::open::is_as_uri(u) { "angelscript" } else { "jass" };
            json!({ "uri": u.to_string(), "languageId": lang })
        }).collect();

        let msg = if errors.is_empty() {
            format!("Rescanned {} files", ok_count)
        } else {
            format!("Rescanned {} files ({} errors)\n{}", ok_count, errors.len(), errors.join("\n"))
        };

        let done = json!({
            "done": true,
            "ok": errors.is_empty(),
            "message": msg,
            "errors": errors,
            "files": scanned_files,
            "entries": uri_entries,
            "root": common_parent.display().to_string(),
        });
        let _ = tx.send(Ok(Event::default().data(done.to_string()))).await;
    });

    // Convert the receiver into a stream for axum SSE.
    let event_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let sse = Sse::new(event_stream)
        .keep_alive(axum::response::sse::KeepAlive::default());

    Ok(sse.into_response())
}

pub async fn rescan_status(
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    Ok(Json(json!({ "running": crate::util::rescan::is_running() })))
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
    }

    Ok(Json(result))
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Language feature endpoints
// ═══════════════════════════════════════════════════════════════════════════════

// ─── Completion ───────────────────────────────────────────────────────────────

pub async fn completion(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::http::completion::CompletionParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let items = crate::http::completion::compute::compute(&params.uri, &params.position);
    let list = crate::http::completion::CompletionList {
        is_incomplete: items.iter().any(|i| i.kind == Some(crate::http::completion::CompletionItemKind::Folder)),
        items,
    };
    Ok(Json(serde_json::to_value(list).unwrap_or_default()))
}


// ─── Combined cursor context (hover + highlight + codeAction) ─────────────────

#[derive(Deserialize)]
pub struct CursorContextParams {
    pub uri: Url,
    pub position: crate::http::position::Position,
    /// Non-zero selection range for code actions. When absent, a zero-width
    /// range at `position` is used and diagnostics are taken from the snapshot.
    #[serde(default)]
    pub range: Option<crate::http::range::Range>,
    #[serde(default)]
    pub context: Option<crate::http::code_action::CodeActionContext>,
}

pub async fn cursor_context(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<CursorContextParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let uri = &params.uri;
    let position = &params.position;

    let hover_result = crate::http::hover::compute(uri, position);
    let highlights = crate::http::highlight::compute_highlight(uri, position);

    // Code actions: use client-provided range/context if present,
    // otherwise build from server-side diagnostics at cursor position.
    let (range, diagnostics) = match (params.range, params.context) {
        (Some(r), Some(ctx)) => (r, ctx.diagnostics),
        _ => {
            let r = crate::http::range::Range {
                start: position.clone(),
                end: position.clone(),
            };
            let diags = crate::util::parse_cache::PARSE_CACHE
                .get(uri)
                .map(|snap| {
                    snap.diagnostics
                        .iter()
                        .filter(|d| {
                            d.range.start.line <= position.line
                                && d.range.end.line >= position.line
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (r, diags)
        }
    };

    let ca_params = crate::http::code_action::CodeActionParams {
        uri: uri.clone(),
        range,
        context: crate::http::code_action::CodeActionContext {
            diagnostics,
            only: None,
        },
    };
    let code_actions = crate::http::code_action::compute::compute(&ca_params);

    Ok(Json(json!({
        "hover": hover_result,
        "highlights": highlights,
        "codeActions": code_actions,
    })))
}

// ─── Definition ───────────────────────────────────────────────────────────────

pub async fn definition(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::http::highlight::DefinitionParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let uri = &params.uri;
    let mut locs = Vec::new();
    if let Some(snapshot) = crate::util::parse_cache::PARSE_CACHE.get(uri) {
        if let Some(rope_entry) = crate::util::roper::uri_map::ROPE_MAP.get(uri) {
            if let Some(byte) = params.position.to_byte_offset(rope_entry.value()) {
                let ref_map = &snapshot.ref_map;
                if let Some(ext) = ref_map.external_at(byte) {
                    for origin in &ext.origins {
                        if let Some(ext_snap) = crate::util::parse_cache::peek_or_load(&origin.uri) {
                            for group in ext_snap.ref_map.groups.values() {
                                if group.name == ext.name {
                                    for occ in &group.occurrences {
                                        if occ.is_decl {
                                            locs.push(Location {
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
                        locs.push(Location {
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
    Json(params): Json<crate::http::highlight::ReferenceParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let uri = &params.uri;
    let mut locs = Vec::new();
    if let Some(snapshot) = crate::util::parse_cache::PARSE_CACHE.get(uri) {
        if let Some(rope_entry) = crate::util::roper::uri_map::ROPE_MAP.get(uri) {
            if let Some(byte) = params.position.to_byte_offset(rope_entry.value()) {
                let include_decl = params.context.include_declaration;
                for occ in snapshot.ref_map.occurrences_at(byte) {
                    if !include_decl && occ.is_decl { continue; }
                    locs.push(Location {
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
    Json(params): Json<crate::http::formatting::DocumentFormattingParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let uri = &params.uri;
    let edits: Vec<crate::http::formatting::TextEdit> = if let Some(lng) = crate::util::uri_map::LNG_URI_MAP.get(uri) {
        match lng.value().as_str() {
            "jass" => crate::http::formatting::jass::format(uri, &params.options),
            "angelscript" => crate::http::formatting::ass::format(uri, &params.options),
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
            if let Some(snap) = crate::util::parse_cache::PARSE_CACHE.get(uri) {
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
    Json(params): Json<crate::http::rename::PrepareRenameParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::http::rename::prepare_rename(&params.uri, &params.position);
    Ok(Json(match result {
        Some(r) => serde_json::to_value(r).unwrap_or(Value::Null),
        None => Value::Null,
    }))
}

// ─── Rename ───────────────────────────────────────────────────────────────────

pub async fn rename(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::http::rename::RenameParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let edit = crate::http::rename::compute_identifier_rename(
        &params.uri, &params.position, &params.new_name,
    );
    Ok(Json(serde_json::to_value(edit).unwrap_or_default()))
}

// ─── Will Rename Files ────────────────────────────────────────────────────────

pub async fn will_rename_files(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::http::file_rename::RenameFilesParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let edit = crate::http::file_rename::compute_rename_edits(&params.files);
    Ok(Json(serde_json::to_value(edit).unwrap_or_default()))
}

// ─── Color Presentation ──────────────────────────────────────────────────────

pub async fn color_presentation(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::http::color::ColorPresentationParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let range_len = if params.range.start.line == params.range.end.line {
        params.range.end.character.saturating_sub(params.range.start.character)
    } else { 0 };
    let is_pipe_color = range_len == 10;
    let presentations = if is_pipe_color {
        let label = crate::lng::string_colors::color_to_pipe_string(&params.color);
        vec![crate::http::color::ColorPresentation {
            label: label.clone(),
            text_edit: Some(crate::http::color::TextEdit { range: params.range.clone(), new_text: label }),
            additional_text_edits: None,
        }]
    } else {
        let label = crate::lng::string_colors::color_to_hex_string(&params.color);
        vec![crate::http::color::ColorPresentation {
            label: label.clone(),
            text_edit: Some(crate::http::color::TextEdit { range: params.range.clone(), new_text: label }),
            additional_text_edits: None,
        }]
    };
    Ok(Json(serde_json::to_value(&presentations).unwrap_or_default()))
}


// ─── Signature Help ───────────────────────────────────────────────────────────

pub async fn signature_help(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::http::signature_help::SignatureHelpParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::http::signature_help::compute(&params.uri, &params.position);
    Ok(Json(match result {
        Some(h) => serde_json::to_value(h).unwrap_or(Value::Null),
        None => Value::Null,
    }))
}

// ─── Call Hierarchy ───────────────────────────────────────────────────────────

pub async fn call_hierarchy_prepare(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::http::call_hierarchy::CallHierarchyPrepareParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::http::call_hierarchy::compute_prepare(&params.uri, &params.position);
    Ok(Json(match result {
        Some(items) => serde_json::to_value(&items).unwrap_or(Value::Null),
        None => Value::Null,
    }))
}

pub async fn call_hierarchy_incoming(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::http::call_hierarchy::CallHierarchyIncomingCallsParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::http::call_hierarchy::compute_incoming(&params.item);
    Ok(Json(serde_json::to_value(&result).unwrap_or_default()))
}

pub async fn call_hierarchy_outgoing(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::http::call_hierarchy::CallHierarchyOutgoingCallsParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::http::call_hierarchy::compute_outgoing(&params.item);
    Ok(Json(serde_json::to_value(&result).unwrap_or_default()))
}

// ─── Type Hierarchy ───────────────────────────────────────────────────────────

pub async fn type_hierarchy_prepare(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::http::type_hierarchy::TypeHierarchyPrepareParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::http::type_hierarchy::compute_prepare(&params.uri, &params.position);
    Ok(Json(match result {
        Some(items) => serde_json::to_value(&items).unwrap_or(Value::Null),
        None => Value::Null,
    }))
}

pub async fn type_hierarchy_supertypes(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::http::type_hierarchy::TypeHierarchySupertypesParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::http::type_hierarchy::compute_supertypes(&params.item);
    Ok(Json(serde_json::to_value(&result).unwrap_or_default()))
}

pub async fn type_hierarchy_subtypes(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::http::type_hierarchy::TypeHierarchySubtypesParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.check()?;
    let result = crate::http::type_hierarchy::compute_subtypes(&params.item);
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
    /// Diagnostics — per diagnostic:
    /// `[u32 startLine][u32 startChar][u32 endLine][u32 endChar]
    ///  [u8 severity][u16 msgLen][…msg UTF-8…]
    ///  [u8 tagCount][u8… tags]
    ///  [u16 codeLen][…code UTF-8…]
    ///  [u16 codeHrefLen][…codeHref UTF-8…]
    ///  [u16 sourceLen][…source UTF-8…]`
    pub const DIAGNOSTICS: u8 = 0x04;
    /// Folding ranges — per range:
    /// `[u32 startLine][u32 endLine][u8 kind]`
    /// kind: 0 = none, 1 = comment, 2 = imports, 3 = region
    pub const FOLDING: u8 = 0x05;
    /// Document symbols — raw JSON array (tree structure too complex for
    /// a flat binary encoding).
    pub const SYMBOLS: u8 = 0x06;
    /// Document links — per link:
    /// `[u32 startLine][u32 startChar][u32 endLine][u32 endChar]
    ///  [u16 targetLen][…target UTF-8…]
    ///  [u16 tooltipLen][…tooltip UTF-8…]`
    pub const LINKS: u8 = 0x07;
    /// Document colors — per color:
    /// `[u32 startLine][u32 startChar][u32 endLine][u32 endChar]
    ///  [f32 red][f32 green][f32 blue][f32 alpha]`
    pub const COLORS: u8 = 0x08;
    /// Code lenses (reference counts) — per lens:
    /// `[u32 declLine][u32 declChar][u32 refCount]
    ///  [u32 refStartLine][u32 refStartChar][u32 refEndLine][u32 refEndChar] × refCount`
    pub const CODE_LENSES: u8 = 0x09;
    /// All URIs in the import tree — per URI:
    /// `[u16 uriLen][…uri UTF-8…]`
    /// Allows the client to mark every file in the tree in the Explorer.
    pub const TREE_URIS: u8 = 0x0B;

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
    /// Comma-separated list of hint types to include in the response.
    /// Empty or missing → no hints.  Example: `hints=ref,type`.
    #[serde(default)]
    pub hints: String,
}

/// Build binary TLV response from the current `PARSE_CACHE` snapshot.
///
/// `prev_result_id` is the client's `lastResultId` — if it matches the
/// server's `SEMANTIC_LAST`, a compact token-aware delta is sent instead of
/// the full token array.
///
/// `hints` controls which inlay hint types to include in the response.
/// Empty string → no hints.  Space/comma-separated tags: `ref`, `type`.
fn build_update_response(uri: &Url, prev_result_id: Option<u32>, hints: &str) -> Vec<u8> {
    use crate::util::parse_cache::PARSE_CACHE;

    let mut buf = Vec::new();

    // Clone the Arc and drop the DashMap guard immediately so we don't
    // hold a shard lock while later acquiring IMPORT_GRAPH locks
    // (IMPORT_GRAPH.update() can call PARSE_CACHE.remove() under its
    // own write lock, causing a lock-ordering deadlock).
    let snap = match PARSE_CACHE.get(uri) {
        Some(s) => std::sync::Arc::clone(s.value()),
        None => return buf,
    };

    // ── Section 0x01/0x03: Semantic tokens (full or delta) ────────
    let semantic = snap.semantic.read().unwrap().data(None);

    // Try to compute a token-aware delta against the previous result.
    let diff: Option<Vec<u32>> = prev_result_id
        .filter(|id| *id != 0)
        .and_then(|client_id| {
            let prev = SEMANTIC_LAST.get(uri)?;
            if prev.0 != client_id { return None; }
            Some(crate::http::diff::semantic_diff(&prev.1, &semantic))
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
    if !hints.is_empty() {
        let all_hints = snap.all_inlay_hints();
        if !all_hints.is_empty() {
            let mut section_buf = Vec::new();
            for hint in &all_hints {
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
    }

    // ── Section 0x04: Diagnostics ─────────────────────────────────
    {
        let diagnostics = &snap.diagnostics;
        let mut section_buf = Vec::new();
        for d in diagnostics {
            // Range: 4 × u32
            section_buf.extend_from_slice(&(d.range.start.line as u32).to_le_bytes());
            section_buf.extend_from_slice(&(d.range.start.character as u32).to_le_bytes());
            section_buf.extend_from_slice(&(d.range.end.line as u32).to_le_bytes());
            section_buf.extend_from_slice(&(d.range.end.character as u32).to_le_bytes());
            // Severity: u8
            section_buf.push(d.severity.map(|s| s as u8).unwrap_or(0));
            // Message: u16 len + UTF-8
            let msg = d.message.as_bytes();
            section_buf.extend_from_slice(&(msg.len() as u16).to_le_bytes());
            section_buf.extend_from_slice(msg);
            // Tags: u8 count + u8 each
            let tags = d.tags.as_deref().unwrap_or(&[]);
            section_buf.push(tags.len() as u8);
            for t in tags {
                section_buf.push(*t as u8);
            }
            // Code: u16 len + UTF-8
            let code_str = d.code.as_ref().map(|c| match c {
                crate::http::diagnostic::DiagnosticCode::String(s) => s.clone(),
                crate::http::diagnostic::DiagnosticCode::Int(i) => i.to_string(),
            }).unwrap_or_default();
            let code_bytes = code_str.as_bytes();
            section_buf.extend_from_slice(&(code_bytes.len() as u16).to_le_bytes());
            section_buf.extend_from_slice(code_bytes);
            // Code href: u16 len + UTF-8
            let code_href = d.code_description.as_ref().map(|cd| cd.href.as_bytes()).unwrap_or(&[]);
            section_buf.extend_from_slice(&(code_href.len() as u16).to_le_bytes());
            section_buf.extend_from_slice(code_href);
            // Source: u16 len + UTF-8
            let source = d.source.as_deref().unwrap_or("").as_bytes();
            section_buf.extend_from_slice(&(source.len() as u16).to_le_bytes());
            section_buf.extend_from_slice(source);
        }
        // Always send (even empty → clears old diagnostics)
        buf.push(section::DIAGNOSTICS);
        buf.extend_from_slice(&(section_buf.len() as u32).to_le_bytes());
        buf.extend_from_slice(&section_buf);
    }

    // ── Section 0x05: Folding ranges ──────────────────────────────
    if !snap.folding.is_empty() {
        let mut section_buf = Vec::with_capacity(snap.folding.len() * 9);
        for fr in &snap.folding {
            section_buf.extend_from_slice(&(fr.start_line as u32).to_le_bytes());
            section_buf.extend_from_slice(&(fr.end_line as u32).to_le_bytes());
            let kind_byte: u8 = match fr.kind.as_ref() {
                Some(crate::http::folding::FoldingRangeKind::Comment) => 1,
                Some(crate::http::folding::FoldingRangeKind::Imports) => 2,
                Some(crate::http::folding::FoldingRangeKind::Region) => 3,
                None => 0,
            };
            section_buf.push(kind_byte);
        }
        buf.push(section::FOLDING);
        buf.extend_from_slice(&(section_buf.len() as u32).to_le_bytes());
        buf.extend_from_slice(&section_buf);
    }

    // ── Section 0x06: Document symbols (JSON) ─────────────────────
    if !snap.symbols.is_empty() {
        let json = serde_json::to_vec(&snap.symbols).unwrap_or_default();
        if !json.is_empty() {
            buf.push(section::SYMBOLS);
            buf.extend_from_slice(&(json.len() as u32).to_le_bytes());
            buf.extend_from_slice(&json);
        }
    }

    // ── Section 0x07: Document links ──────────────────────────────
    if !snap.links.is_empty() {
        let mut section_buf = Vec::new();
        for link in &snap.links {
            section_buf.extend_from_slice(&(link.range.start.line as u32).to_le_bytes());
            section_buf.extend_from_slice(&(link.range.start.character as u32).to_le_bytes());
            section_buf.extend_from_slice(&(link.range.end.line as u32).to_le_bytes());
            section_buf.extend_from_slice(&(link.range.end.character as u32).to_le_bytes());
            let target = link.target.as_deref().unwrap_or("").as_bytes();
            section_buf.extend_from_slice(&(target.len() as u16).to_le_bytes());
            section_buf.extend_from_slice(target);
            let tooltip = link.tooltip.as_deref().unwrap_or("").as_bytes();
            section_buf.extend_from_slice(&(tooltip.len() as u16).to_le_bytes());
            section_buf.extend_from_slice(tooltip);
        }
        buf.push(section::LINKS);
        buf.extend_from_slice(&(section_buf.len() as u32).to_le_bytes());
        buf.extend_from_slice(&section_buf);
    }

    // ── Section 0x08: Document colors ─────────────────────────────
    if !snap.colors.is_empty() {
        let mut section_buf = Vec::with_capacity(snap.colors.len() * 32);
        for ci in &snap.colors {
            section_buf.extend_from_slice(&(ci.range.start.line as u32).to_le_bytes());
            section_buf.extend_from_slice(&(ci.range.start.character as u32).to_le_bytes());
            section_buf.extend_from_slice(&(ci.range.end.line as u32).to_le_bytes());
            section_buf.extend_from_slice(&(ci.range.end.character as u32).to_le_bytes());
            section_buf.extend_from_slice(&(ci.color.red as f32).to_le_bytes());
            section_buf.extend_from_slice(&(ci.color.green as f32).to_le_bytes());
            section_buf.extend_from_slice(&(ci.color.blue as f32).to_le_bytes());
            section_buf.extend_from_slice(&(ci.color.alpha as f32).to_le_bytes());
        }
        buf.push(section::COLORS);
        buf.extend_from_slice(&(section_buf.len() as u32).to_le_bytes());
        buf.extend_from_slice(&section_buf);
    }

    // ── Section 0x09: Code lenses (reference counts) ────────────
    {
        let lens_value = snap.file_symbols.file_settings
            .get("lens")
            .map(|v| v.as_str())
            .unwrap_or("");
        let lens_fn = lens_value.split_whitespace().any(|w| w == "fn");
        let lens_var = lens_value.split_whitespace().any(|w| w == "var");
        let lens_arg = lens_value.split_whitespace().any(|w| w == "arg");

        if lens_fn || lens_var || lens_arg {
            let ref_map = &snap.ref_map;
            let mut lenses: Vec<_> = ref_map.groups.iter()
                .filter_map(|(&key, group)| {
                    let dominated = if snap.func_decl_keys.contains(&key) {
                        lens_fn
                    } else if snap.arg_decl_keys.contains(&key) {
                        lens_arg
                    } else if snap.var_decl_keys.contains(&key) {
                        lens_var
                    } else {
                        false
                    };
                    if !dominated { return None; }
                    let decl = group.occurrences.iter().find(|o| o.is_decl)?;
                    let refs: Vec<_> = group.occurrences.iter().filter(|o| !o.is_decl).collect();
                    Some((decl.range.clone(), refs))
                })
                .collect();
            lenses.sort_by_key(|(r, _)| (r.start.line, r.start.character));

            if !lenses.is_empty() {
                let mut section_buf = Vec::new();
                for (decl_range, refs) in &lenses {
                    section_buf.extend_from_slice(&(decl_range.start.line as u32).to_le_bytes());
                    section_buf.extend_from_slice(&(decl_range.start.character as u32).to_le_bytes());
                    section_buf.extend_from_slice(&(refs.len() as u32).to_le_bytes());
                    for r in refs {
                        section_buf.extend_from_slice(&(r.range.start.line as u32).to_le_bytes());
                        section_buf.extend_from_slice(&(r.range.start.character as u32).to_le_bytes());
                        section_buf.extend_from_slice(&(r.range.end.line as u32).to_le_bytes());
                        section_buf.extend_from_slice(&(r.range.end.character as u32).to_le_bytes());
                    }
                }
                buf.push(section::CODE_LENSES);
                buf.extend_from_slice(&(section_buf.len() as u32).to_le_bytes());
                buf.extend_from_slice(&section_buf);
            }
        }
    }

    // ── Section 0x0B: All tree URIs ──────────────────────────────
    {
        use crate::util::import_graph::IMPORT_GRAPH;
        let tree_uris = IMPORT_GRAPH.tree_for_uri(uri);
        {
            let mut tree_buf = Vec::new();
            for peer in &tree_uris {
                let uri_bytes = peer.as_str().as_bytes();
                tree_buf.extend_from_slice(&(uri_bytes.len() as u16).to_le_bytes());
                tree_buf.extend_from_slice(uri_bytes);
            }
            if !tree_buf.is_empty() {
                buf.push(section::TREE_URIS);
                buf.extend_from_slice(&(tree_buf.len() as u32).to_le_bytes());
                buf.extend_from_slice(&tree_buf);
            }
        }
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


/// Parse content changes from binary TLV sections (type 0x11).
fn parse_content_changes(body: &[u8]) -> Result<Vec<crate::util::change::TextDocumentContentChangeEvent>, String> {
    use crate::http::position::Position;
    use crate::http::range::Range;

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

        changes.push(crate::util::change::TextDocumentContentChangeEvent {
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

    // Build a response that only contains the echoed version prefix (no TLV
    // sections).  Used when there is nothing to return — unrecognised
    // language, empty body, etc.
    let empty = |v: u32| Ok((
        [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
        v.to_le_bytes().to_vec(),
    ));


    // ── Parse TLV request body ───────────────────────────────────
    let mut has_work = false;
    let fname = uri.path().rsplit('/').next().unwrap_or("?");
    debug_log!("[update] start lang={} file={}", lang, fname);

    if body.len() >= 5 && body[0] == section::OPEN_URI {
        // ── Open by URI — server reads from disk ──────────────────
        debug_log!("[update] OPEN_URI reading from disk");
        let file_path = uri.to_file_path()
            .map_err(|_| (StatusCode::BAD_REQUEST, "URI is not a file:// path".into()))?;
        let text = tokio::fs::read_to_string(&file_path).await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("read {}: {e}", file_path.display())))?;
        debug_log!("[update] read {} bytes", text.len());

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
        crate::util::import_graph::IMPORT_GRAPH.ensure_node_pub(&uri);
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
        crate::util::import_graph::IMPORT_GRAPH.ensure_node_pub(&uri);
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
        debug_log!("[update] no work, returning empty");
        return empty(version);
    }

    // ── Parse ─────────────────────────────────────────────────────
    // The client-side serial queue guarantees only one in-flight request
    // per URI, so no server-side cancellation race is needed.
    debug_log!("[update] starting parse");
    let parse_gen = crate::util::parse_cache::mark_parse_pending(&uri);
    let res = match lang {
        "bni" => crate::lng::bni::parse::parse_and_notify(&uri).await,
        "jass" => crate::lng::jass::parse::parse_and_notify(&uri, Some(parse_gen)).await,
        "angelscript" => crate::lng::ass::parse::parse_and_notify(&uri, Some(parse_gen)).await,
        "wts" => crate::lng::wts::parse::parse_and_notify(&uri).await,
        "slk" => crate::lng::slk::parse::parse_and_notify(&uri).await,
        _ => Ok(()),
    };
    if let Err(e) = res {
        log::error!("{} parse: {}", lang, e);
        debug_log!("[update] parse ERROR: {}", e);
    }
    crate::util::parse_cache::mark_parse_done(&uri, parse_gen);
    debug_log!("[update] parse done, building response");

    // ── Build binary TLV response ────────────────────────────────
    // The first 4 bytes are the echoed client version (u32 LE) so the
    // client can discard stale responses that no longer match its
    // current document state.
    let tlv = build_update_response(&uri, params.last_result_id, &params.hints);
    let mut resp = Vec::with_capacity(4 + tlv.len());
    resp.extend_from_slice(&version.to_le_bytes());
    resp.extend_from_slice(&tlv);
    Ok(([(axum::http::header::CONTENT_TYPE, "application/octet-stream")], resp))
}

pub async fn document_close(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<DidCloseTextDocumentParams>,
) -> Result<StatusCode, (StatusCode, String)> {
    auth.check()?;
    let uri = params.text_document.uri;
    SEMANTIC_LAST.remove(&uri);
    crate::util::parse_cache::evict_closed_file(&uri);
    Ok(StatusCode::OK)
}

pub async fn files_changed(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<DidChangeWatchedFilesParams>,
) -> Result<StatusCode, (StatusCode, String)> {
    auth.check()?;

    use crate::util::import_graph::IMPORT_GRAPH;
    use crate::util::parse_cache::PARSE_CACHE;

    let mut dependents_to_reparse: std::collections::HashSet<Url> =
        std::collections::HashSet::new();

    for event in &params.changes {
        let changed_uri = &event.uri;

        if event.change_type == 3 {
            PARSE_CACHE.remove(changed_uri);
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
        });
    }

    Ok(StatusCode::OK)
}

// ─── Did Rename Files (swap URI, no rescan) ───────────────────────────────────

pub async fn did_rename_files(
    Query(auth): Query<AuthQuery>,
    Json(params): Json<crate::http::file_rename::RenameFilesParams>,
) -> Result<StatusCode, (StatusCode, String)> {
    auth.check()?;

    use crate::util::import_graph::IMPORT_GRAPH;
    use crate::util::roper::uri_map::ROPE_MAP;
    use crate::util::parse_cache::PARSE_CACHE;

    for rename in &params.files {
        let old_url = &rename.old_uri;
        let new_url = &rename.new_uri;

        // Swap URI in ROPE_MAP
        if let Some((_, rope)) = ROPE_MAP.remove(old_url) {
            ROPE_MAP.insert(new_url.clone(), rope);
        }

        // Swap URI in PARSE_CACHE
        if let Some((_, snap)) = PARSE_CACHE.remove(old_url) {
            PARSE_CACHE.insert(new_url.clone(), snap);
        }

        // Swap URI in SEMANTIC_LAST
        if let Some((_, sem)) = SEMANTIC_LAST.remove(old_url) {
            SEMANTIC_LAST.insert(new_url.clone(), sem);
        }

        // Swap URI in IMPORT_GRAPH
        IMPORT_GRAPH.rename_node(old_url, new_url);
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

    let data = match crate::util::parse_cache::PARSE_CACHE.get(&uri) {
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

// ─── SSE debug log ───────────────────────────────────────────────────────────

pub async fn sse_debug_log(
    Query(auth): Query<AuthQuery>,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    check_token(&TokenParam { token: auth.token })?;

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::response::sse::Event, std::convert::Infallible>>(64);

    tokio::spawn(async move {
        let mut rx_log = crate::util::debug_log::subscribe();
        loop {
            match rx_log.recv().await {
                Ok(msg) => {
                    let event = axum::response::sse::Event::default().data(msg);
                    if tx.send(Ok(event)).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Ok(axum::response::sse::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default()))
}

use tokio::sync::broadcast;
