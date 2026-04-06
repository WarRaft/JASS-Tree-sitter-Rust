pub(crate) mod http;
pub(crate) mod lsp;
pub(crate) mod util;

pub(crate) mod lng;

use serde_json::json;
use tokio::sync::mpsc;

use crate::lng::blp::send::send as blp_send;
use crate::lng::doo::send::send as doo_send;
use crate::lng::mdx::send::send as mdx_send;
use crate::lng::slk::send::send as slk_send;
use crate::lng::w3abdhqtu::send::send as w3obj_send;
use crate::lng::w3e::send::send as w3e_send;
use crate::lng::w3i::send::send as w3i_send;
use crate::lng::mpq::send::{send_info as mpq_info_send, send_list as mpq_list_send, send_read as mpq_read_send};
use crate::lsp::cancel::CancelCheck;
use crate::lsp::code_action::send::send as code_action_send;
use crate::lsp::code_lens::send::send as code_lens_send;
use crate::lsp::completion::send::send as completion_send;
use crate::lsp::formatting::send::send_formatting;
use crate::lsp::highlight::send::send as highlight_send;
use crate::lsp::hover::send::send as hover_send;
use crate::lsp::protocol::{LspMessage, MethodCall, ResponseMessage};
use crate::lsp::rename::handle::compute_rename_edits;
use crate::lsp::rename::identifier::{compute_identifier_rename, prepare_rename};
use crate::lsp::send::send;
use crate::lsp::send::send_cancelled;
use crate::lsp::signature_help::send::send as signature_help_send;
use crate::util::file_store::{
    cancel_uri_requests, mark_parse_pending, mark_parse_done,
    uri_request_token, FILE_STORE,
};
use crate::util::uri_map::LNG_URI_MAP;
use log::{error, info};
use tokio_util::sync::CancellationToken;
use url::Url;


/// Extract the document URI from a `MethodCall` (if it has one).
///
/// Used to obtain the per-URI request cancellation token **before** the
/// payload is moved into the spawned handler.
fn extract_uri(call: &MethodCall) -> Option<&Url> {
    match call {
        MethodCall::Completion(p) => Some(&p.text_document.uri),
        MethodCall::Hover(p) => Some(&p.text_document.uri),
        MethodCall::DocumentHighlight(p) => Some(&p.text_document.uri),
        MethodCall::Definition(p) => Some(&p.text_document.uri),
        MethodCall::References(p) => Some(&p.text_document.uri),
        MethodCall::Formatting(p) => Some(&p.text_document.uri),
        MethodCall::PrepareRename(p) => Some(&p.text_document.uri),
        MethodCall::Rename(p) => Some(&p.text_document.uri),
        MethodCall::ColorPresentation(p) => Some(&p.text_document.uri),
        MethodCall::CodeAction(p) => Some(&p.text_document.uri),
        MethodCall::SignatureHelp(p) => Some(&p.text_document.uri),
        MethodCall::CodeLens(p) => Some(&p.text_document.uri),
        MethodCall::PrepareCallHierarchy(p) => Some(&p.text_document.uri),
        MethodCall::PrepareTypeHierarchy(p) => Some(&p.text_document.uri),
        _ => None,
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    // ── Start HTTP + WebSocket server ────────────────────────────
    let http_port = crate::http::server::start_server().await.ok();

    // ── Print port + token to stdout so the extension can connect ─
    if let (Some(port), Some(info)) = (http_port, crate::http::server::BINARY_SERVER.get()) {
        let startup = json!({
            "port": port,
            "token": &info.token,
        });
        println!("{}", startup);
        // Flush to ensure the extension reads it immediately.
        use std::io::Write;
        std::io::stdout().flush().ok();
    }

    // ── Eagerly build snapshot if game path already configured ──
    tokio::task::spawn_blocking(|| {
        let gp = crate::lng::w3e::game_path::get_game_path();
        if !gp.is_empty() {
            log::info!("Game path found on startup, building snapshot…");
            crate::lng::w3e::snapshot::build_snapshot(None);
        }
    });

    // ── Create channel for incoming WebSocket messages ───────────
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<String>();
    let _ = crate::http::ws::MSG_TX.set(msg_tx);

    // ── Stdin watcher: when extension dies stdin closes → we exit ─
    tokio::spawn(async {
        use tokio::io::AsyncReadExt;
        let mut stdin = tokio::io::stdin();
        let mut buf = [0u8; 64];
        loop {
            match stdin.read(&mut buf).await {
                Ok(0) | Err(_) => {
                    log::info!("stdin closed, shutting down");
                    std::process::exit(0);
                }
                Ok(_) => {} // ignore any data
            }
        }
    });

    // ── Main dispatch loop — reads from WebSocket channel ────────
    loop {
        let msg = match msg_rx.recv().await {
            Some(msg) => msg,
            None => break,
        };

        let parsed = match serde_json::from_str::<LspMessage>(&msg) {
            Ok(p) => p,
            Err(err) => {
                error!("Failed to parse message: {} |{}", err, msg);
                continue;
            }
        };


        match parsed {
            LspMessage::Call(call) => {

                match call.payload {

                // ─── Notifications processed inline to preserve ordering ─────

                MethodCall::Cancel(params) => {
                    params.id.mark_cancelled().await;
                }


                MethodCall::DidClose(params) => {
                    let uri = params.text_document.uri;

                    // Cancel in-flight request handlers — they target a now-closed file.
                    cancel_uri_requests(&uri);

                    let evicted = crate::util::file_store::evict_closed_file(&uri);

                    // Send empty parseResult for every evicted URI so the
                    // extension clears stale markers, hints, etc.
                    if !evicted.is_empty() {
                        tokio::spawn(async move {
                            for evicted_uri in &evicted {
                                crate::lsp::send::send(
                                    &json!({
                                        "jsonrpc": "2.0",
                                        "method": "custom/parseResult",
                                        "params": {
                                            "uri": evicted_uri.to_string(),
                                            "semanticTokens": [],
                                            "diagnostics": [],
                                            "inlayHints": [],
                                            "folding": [],
                                            "symbols": [],
                                            "documentLinks": [],
                                            "colors": []
                                        }
                                    }),
                                ).await;
                            }
                        });
                    }
                }


                MethodCall::DidOpen(params) => {
                    if params.text_document.language_id == "bni" {
                        let text = params.text_document.text;
                        let uri = params.text_document.uri;
                        if let Err(e) = lng::bni::open::init(&uri, &text) {
                            error!("bni init: {}", e);
                        } else {
                            let parse_gen = mark_parse_pending(&uri);
                            tokio::spawn(async move {
                                if let Err(e) = lng::bni::parse::parse_and_notify(&uri).await {
                                    error!("bni parse: {}", e);
                                }
                                mark_parse_done(&uri, parse_gen);
                            });
                        }
                    } else if params.text_document.language_id == "jass" {
                        let text = params.text_document.text;
                        let uri = params.text_document.uri;
                        if let Err(e) = lng::jass::open::init(&uri, &text) {
                            error!("jass init: {}", e);
                        } else {
                            let parse_gen = mark_parse_pending(&uri);
                            tokio::spawn(async move {
                                if let Err(e) = lng::jass::parse::parse_and_notify(&uri, Some(parse_gen)).await {
                                    error!("jass parse: {}", e);
                                }
                                mark_parse_done(&uri, parse_gen);
                            });
                        }
                    } else if params.text_document.language_id == "angelscript" {
                        let text = params.text_document.text;
                        let uri = params.text_document.uri;
                        if let Err(e) = lng::ass::open::init(&uri, &text) {
                            error!("as init: {}", e);
                        } else {
                            let parse_gen = mark_parse_pending(&uri);
                            tokio::spawn(async move {
                                if let Err(e) = lng::ass::parse::parse_and_notify(&uri, Some(parse_gen)).await {
                                    error!("as parse: {}", e);
                                }
                                mark_parse_done(&uri, parse_gen);
                            });
                        }
                    } else if params.text_document.language_id == "wts" {
                        let text = params.text_document.text;
                        let uri = params.text_document.uri;
                        if let Err(e) = lng::wts::open::init(&uri, &text) {
                            error!("wts init: {}", e);
                        } else {
                            let parse_gen = mark_parse_pending(&uri);
                            tokio::spawn(async move {
                                if let Err(e) = lng::wts::parse::parse_and_notify(&uri).await {
                                    error!("wts parse: {}", e);
                                }
                                mark_parse_done(&uri, parse_gen);
                            });
                        }
                    } else if params.text_document.language_id == "slk" {
                        let text = params.text_document.text;
                        let uri = params.text_document.uri;
                        if let Err(e) = lng::slk::open::init(&uri, &text) {
                            error!("slk init: {}", e);
                        } else {
                            let parse_gen = mark_parse_pending(&uri);
                            tokio::spawn(async move {
                                if let Err(e) = lng::slk::parse::parse_and_notify(&uri).await {
                                    error!("slk parse: {}", e);
                                }
                                mark_parse_done(&uri, parse_gen);
                            });
                        }
                    }
                }

                MethodCall::DidChange(params) => {
                    let uri = params.text_document.uri;

                    // Cancel all in-flight request handlers for this URI —
                    // they're working with stale data.
                    cancel_uri_requests(&uri);

                    if let Some(lng) = LNG_URI_MAP.get(&uri) {
                        let lng_val = lng.value().clone();
                        drop(lng); // release DashMap guard before calling apply_edits

                        if lng_val == "bni" {
                            if let Err(e) = lng::bni::change::apply_edits(&uri, params.content_changes) {
                                error!("bni edit: {}", e);
                            } else {
                                let parse_gen = mark_parse_pending(&uri);
                                tokio::spawn(async move {
                                    if let Err(e) = lng::bni::parse::parse_and_notify(&uri).await {
                                        error!("bni parse: {}", e);
                                    }
                                    mark_parse_done(&uri, parse_gen);
                                });
                            }
                        } else if lng_val == "jass" {
                            if let Err(e) = lng::jass::change::apply_edits(&uri, params.content_changes) {
                                error!("jass edit: {}", e);
                            } else {
                                let parse_gen = mark_parse_pending(&uri);
                                tokio::spawn(async move {
                                    if let Err(e) = lng::jass::parse::parse_and_notify(&uri, Some(parse_gen)).await {
                                        error!("jass parse: {}", e);
                                    }
                                    mark_parse_done(&uri, parse_gen);
                                });
                            }
                        } else if lng_val == "angelscript" {
                            if let Err(e) = lng::ass::change::apply_edits(&uri, params.content_changes) {
                                error!("as edit: {}", e);
                            } else {
                                let parse_gen = mark_parse_pending(&uri);
                                tokio::spawn(async move {
                                    if let Err(e) = lng::ass::parse::parse_and_notify(&uri, Some(parse_gen)).await {
                                        error!("as parse: {}", e);
                                    }
                                    mark_parse_done(&uri, parse_gen);
                                });
                            }
                        } else if lng_val == "wts" {
                            if let Err(e) = lng::wts::change::apply_edits(&uri, params.content_changes) {
                                error!("wts edit: {}", e);
                            } else {
                                let parse_gen = mark_parse_pending(&uri);
                                tokio::spawn(async move {
                                    if let Err(e) = lng::wts::parse::parse_and_notify(&uri).await {
                                        error!("wts parse: {}", e);
                                    }
                                    mark_parse_done(&uri, parse_gen);
                                });
                            }
                        } else if lng_val == "slk" {
                            if let Err(e) = lng::slk::change::apply_edits(&uri, params.content_changes) {
                                error!("slk edit: {}", e);
                            } else {
                                let parse_gen = mark_parse_pending(&uri);
                                tokio::spawn(async move {
                                    if let Err(e) = lng::slk::parse::parse_and_notify(&uri).await {
                                        error!("slk parse: {}", e);
                                    }
                                    mark_parse_done(&uri, parse_gen);
                                });
                            }
                        }
                    }
                }

                MethodCall::DidChangeWatchedFiles(params) => {
                    // Files changed on disk (created / modified / deleted) — but
                    // NOT via the editor.  Re-parse every file that imports any of
                    // the changed URIs so that diagnostics / links update.
                    use crate::util::import_graph::IMPORT_GRAPH;

                    let mut dependents_to_reparse: std::collections::HashSet<Url> =
                        std::collections::HashSet::new();

                    for event in &params.changes {
                        let changed_uri = &event.uri;

                        // 3 = Deleted: also evict from FILE_STORE / import graph.
                        if event.change_type == 3 {
                            FILE_STORE.remove(changed_uri);
                        }

                        // 1 = Created, 2 = Changed: the file was added or
                        // modified outside the editor.  If it's already known
                        // to the import graph, re-parse it from disk so its
                        // symbols are updated.
                        if event.change_type == 1 || event.change_type == 2 {
                            if IMPORT_GRAPH.all_uris().contains(changed_uri) {
                                dependents_to_reparse.insert(changed_uri.clone());
                            }
                        }

                        // All direct dependents of the changed file need re-parsing.
                        for dep in IMPORT_GRAPH.direct_dependents(changed_uri) {
                            dependents_to_reparse.insert(dep);
                        }
                    }

                    if !dependents_to_reparse.is_empty() {
                        tokio::spawn(async move {
                            for uri in &dependents_to_reparse {
                                // Skip files that are currently open — they
                                // have fresh in-memory state from DidChange.
                                if crate::util::roper::uri_map::ROPE_MAP.contains_key(uri) {
                                    continue;
                                }
                                if let Ok(path) = uri.to_file_path() {
                                    if let Ok(content) = std::fs::read_to_string(&path) {
                                        if let Err(e) = crate::util::open::open_by_uri(uri, &content).await {
                                            error!("file-watcher reparse {}: {}", uri, e);
                                        }
                                    }
                                }
                            }
                            crate::util::file_store::send_refresh_all().await;
                        });
                    }
                }

                // ─── All other methods spawned as concurrent request handlers ─

                other => {
                    // Obtain the per-URI cancellation token BEFORE moving
                    // the payload into the spawned task.  When the next
                    // `didChange` for this URI arrives, the token will be
                    // cancelled and the handler bails out immediately.
                    let ct: Option<CancellationToken> =
                        extract_uri(&other).map(|u| uri_request_token(u));

                    tokio::spawn(async move {

                        // ── Early cancellation check ──────────────────────
                        if let Some(ref ct) = ct {
                            if ct.is_cancelled() || call.id.was_cancelled().await {
                                send_cancelled(call.id).await;
                                return;
                            }
                        }

                        match other {
                            MethodCall::BlpRender(param) => {
                                blp_send(call.id, &param.uri).await;
                            }

                            MethodCall::MdxRender(param) => {
                                mdx_send(call.id, &param.uri).await;
                            }

                            MethodCall::DooRender(param) => {
                                doo_send(call.id, &param.uri, param.is_unit, param.archive_path.as_deref()).await;
                            }

                            MethodCall::W3iRender(param) => {
                                w3i_send(call.id, &param.uri, param.archive_path.as_deref()).await;
                            }

                            MethodCall::W3eRender(param) => {
                                w3e_send(call.id, &param.uri, param.archive_path.as_deref()).await;
                            }

                            MethodCall::W3ObjRender(param) => {
                                w3obj_send(call.id, &param.uri, param.level_data, param.archive_path.as_deref()).await;
                            }

                            MethodCall::W3eGamePathSet(param) => {
                                crate::lng::w3e::game_path::set_game_path(&param.game_path);
                                let status = crate::lng::w3e::game_path::build_status();
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(serde_json::to_value(status).unwrap_or_default()),
                                        error: None,
                                    },
                                ).await;
                            }

                            MethodCall::W3eGamePathStatus(_) => {
                                let status = crate::lng::w3e::game_path::build_status();
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(serde_json::to_value(status).unwrap_or_default()),
                                        error: None,
                                    },
                                ).await;
                            }

                            MethodCall::W3eTerrainSlk(param) => {
                                let ap = param.archive_path.clone();
                                let slk = tokio::task::spawn_blocking(move || {
                                    crate::lng::w3e::slk::load_terrain_slk(ap.as_deref())
                                })
                                .await
                                .ok()
                                .flatten();
                                let result_val = match slk {
                                    Some(data) => serde_json::to_value(data).unwrap_or_default(),
                                    None => serde_json::json!(null),
                                };
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(result_val),
                                        error: None,
                                    },
                                ).await;
                            }

                            MethodCall::W3eDoodadsSlk(param) => {
                                let ap = param.archive_path.clone();
                                let slk = tokio::task::spawn_blocking(move || {
                                    crate::lng::w3e::slk::load_doodads_slk(ap.as_deref())
                                })
                                .await
                                .ok()
                                .flatten();
                                let result_val = match slk {
                                    Some(data) => serde_json::to_value(data).unwrap_or_default(),
                                    None => serde_json::json!(null),
                                };
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(result_val),
                                        error: None,
                                    },
                                ).await;
                            }

                            MethodCall::W3eUnitsSlk(param) => {
                                let ap = param.archive_path.clone();
                                let slk = tokio::task::spawn_blocking(move || {
                                    crate::lng::w3e::slk::load_units_slk(ap.as_deref())
                                })
                                .await
                                .ok()
                                .flatten();
                                let result_val = match slk {
                                    Some(data) => serde_json::to_value(data).unwrap_or_default(),
                                    None => serde_json::json!(null),
                                };
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(result_val),
                                        error: None,
                                    },
                                ).await;
                            }

                            MethodCall::W3eDestructablesSlk(param) => {
                                let ap = param.archive_path.clone();
                                let slk = tokio::task::spawn_blocking(move || {
                                    crate::lng::w3e::slk::load_destructables_slk(ap.as_deref())
                                })
                                .await
                                .ok()
                                .flatten();
                                let result_val = match slk {
                                    Some(data) => serde_json::to_value(data).unwrap_or_default(),
                                    None => serde_json::json!(null),
                                };
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(result_val),
                                        error: None,
                                    },
                                ).await;
                            }

                            MethodCall::W3eLookupFile(param) => {
                                let path = param.path.clone();
                                let ap = param.archive_path.clone();
                                let result = tokio::task::spawn_blocking(move || {
                                    crate::lng::w3e::file_lookup::lookup_file_resolved(&path, ap.as_deref())
                                })
                                .await
                                .ok()
                                .flatten();
                                let result_val = match result {
                                    Some((buf, source, resolved_path)) => {
                                        use base64::Engine;
                                        serde_json::json!({
                                            "content": base64::engine::general_purpose::STANDARD.encode(&buf),
                                            "source": source,
                                            "resolvedPath": resolved_path,
                                        })
                                    }
                                    None => serde_json::json!(null),
                                };
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(result_val),
                                        error: None,
                                    },
                                ).await;
                            }

                            MethodCall::SlkRender(param) => {
                                slk_send(call.id, &param.uri).await;
                            }

                            MethodCall::SlkEdit(param) => {
                                let result = crate::lng::slk::edit::apply_cell_edit(&param);
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(serde_json::json!(result)),
                                        error: None,
                                    },
                                )
                                .await;
                            }


                            MethodCall::Completion(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                completion_send(call.id, uri, &params.position).await;
                            }

                            MethodCall::Hover(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                hover_send(call.id, uri, &params.position).await;
                            }

                            MethodCall::DocumentHighlight(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(call.id).await;
                                    return;
                                }
                                highlight_send(call.id, &params).await;
                            }

                            MethodCall::Definition(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;

                                let result = {
                                    let mut locs = Vec::new();
                                    if let Some(snapshot) = FILE_STORE.get(uri) {
                                        if let Some(rope_entry) = crate::util::roper::uri_map::ROPE_MAP.get(uri) {
                                            if let Some(byte) = params.position.to_byte_offset(rope_entry.value()) {
                                                let ref_map = &snapshot.ref_map;
                                                if let Some(ext) = ref_map.external_at(byte) {
                                                    // Cross-file: look up declarations in ALL origin files.
                                                    for origin in &ext.origins {
                                                        if let Some(ext_snap) = FILE_STORE.get(&origin.uri) {
                                                            let ext_ref_map = &ext_snap.ref_map;
                                                            for group in ext_ref_map.groups.values() {
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
                                    locs
                                };

                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(&result),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::References(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;

                                let result = {
                                    let mut locs = Vec::new();
                                    if let Some(snapshot) = FILE_STORE.get(uri) {
                                        if let Some(rope_entry) = crate::util::roper::uri_map::ROPE_MAP.get(uri) {
                                            if let Some(byte) = params.position.to_byte_offset(rope_entry.value()) {
                                                let include_decl = params.context.include_declaration;
                                                for occ in snapshot.ref_map.occurrences_at(byte) {
                                                    if !include_decl && occ.is_decl {
                                                        continue;
                                                    }
                                                    locs.push(crate::lsp::location::Location {
                                                        uri: uri.to_string(),
                                                        range: occ.range.clone(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                    locs
                                };

                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(&result),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::Formatting(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(call.id).await;
                                    return;
                                }
                                send_formatting(call.id, &params).await;
                            }

                            MethodCall::PrepareRename(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;

                                let result = prepare_rename(uri, &params.position);
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(match result {
                                            Some(r) => serde_json::to_value(r)
                                                .unwrap_or(serde_json::Value::Null),
                                            None => serde_json::Value::Null,
                                        }),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::Rename(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;

                                let edit = compute_identifier_rename(
                                    uri,
                                    &params.position,
                                    &params.new_name,
                                );
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(edit),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::WillRenameFiles(params) => {
                                let edit = compute_rename_edits(&params.files);
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(edit),
                                        error: None,
                                    },
                                )
                                .await;
                            }


                            MethodCall::ColorPresentation(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(call.id).await;
                                    return;
                                }
                                crate::lsp::color::send::color_presentation_send(
                                    call.id, &params,
                                ).await;
                            }

                            MethodCall::ImportGraphSubgraph(params) => {
                                let uri = &params.uri;
                                let (nodes, edges) =
                                    crate::util::import_graph::IMPORT_GRAPH
                                        .subgraph_for(uri);
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(json!({
                                            "uri": uri.to_string(),
                                            "nodes": nodes,
                                            "edges": edges,
                                        })),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::CallGraphSubgraph(params) => {
                                let uri = &params.uri;
                                let result =
                                    crate::util::call_graph::build_call_graph(uri);
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(json!(result)),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::TypeGraphSubgraph(params) => {
                                let uri = &params.uri;
                                let result =
                                    crate::util::type_graph::build_type_graph(uri);
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(json!(result)),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::RescanExecute(params) => {
                                use crate::util::file_cache;
                                use crate::util::import_graph::IMPORT_GRAPH;
                                use crate::util::scope_resolver::SCOPE_RESOLVER;

                                let uri = &params.uri;

                                // ── 1. Resolve the tree for this URI ──────────
                                let tree_uris = IMPORT_GRAPH.tree_for_uri(uri);

                                if tree_uris.is_empty() {
                                    send(
                                        &ResponseMessage {
                                            jsonrpc: "2.0".into(),
                                            id: call.id,
                                            result: Some(json!({
                                                "ok": false,
                                                "message": "No files in tree"
                                            })),
                                            error: None,
                                        },
                                    ).await;
                                    return;
                                }

                                let total = tree_uris.len();
                                let tree_list: Vec<url::Url> = tree_uris.iter().cloned().collect();

                                info!(
                                    "rescan: tree rescan for {} — {} file(s)",
                                    uri.path().rsplit('/').next().unwrap_or(""),
                                    total
                                );

                                // ── 2. Purge caches for this tree only ────────
                                file_cache::purge_set(&tree_uris);
                                SCOPE_RESOLVER.remove_files(&tree_uris);
                                for u in &tree_list {
                                    FILE_STORE.remove(u);
                                }

                                // ── 3. Re-parse tree files with progress ──────
                                let token = "jass-rescan-tree";

                                send(
                                    &json!({
                                        "jsonrpc": "2.0",
                                        "id": 99998,
                                        "method": "window/workDoneProgress/create",
                                        "params": { "token": token }
                                    }),
                                ).await;

                                send(
                                    &json!({
                                        "jsonrpc": "2.0",
                                        "method": "$/progress",
                                        "params": {
                                            "token": token,
                                            "value": {
                                                "kind": "begin",
                                                "title": "JASS: Rescan tree",
                                                "cancellable": false,
                                                "percentage": 0
                                            }
                                        }
                                    }),
                                ).await;

                                let mut ok_count = 0usize;
                                let mut errors: Vec<String> = Vec::new();

                                for (i, u) in tree_list.iter().enumerate() {
                                    let fname = u.path().rsplit('/').next().unwrap_or("");
                                    let pct = ((i + 1) * 100 / total) as u32;
                                    info!("rescan {}/{} {}", i + 1, total, fname);

                                    send(
                                        &json!({
                                            "jsonrpc": "2.0",
                                            "method": "$/progress",
                                            "params": {
                                                "token": token,
                                                "value": {
                                                    "kind": "report",
                                                    "message": format!("{}/{} {}", i + 1, total, fname),
                                                    "percentage": pct
                                                }
                                            }
                                        }),
                                    ).await;

                                    match u.to_file_path() {
                                        Ok(path) if path.is_dir() => {
                                            continue;
                                        }
                                        Ok(path) => match std::fs::read_to_string(&path) {
                                            Ok(content) => {
                                                if let Err(e) = crate::util::open::open_by_uri(u, &content).await {
                                                    let msg = format!("{}: {}", fname, e);
                                                    error!("rescan {}", msg);
                                                    errors.push(msg);
                                                } else {
                                                    ok_count += 1;
                                                }
                                            }
                                            Err(e) => {
                                                let msg = format!("{}: cannot read — {}", fname, e);
                                                error!("rescan {}", msg);
                                                errors.push(msg);
                                            }
                                        },
                                        Err(_) => {
                                            let msg = format!("{}: invalid file path", fname);
                                            error!("rescan {}", msg);
                                            errors.push(msg);
                                        }
                                    }
                                }

                                send(
                                    &json!({
                                        "jsonrpc": "2.0",
                                        "method": "$/progress",
                                        "params": {
                                            "token": token,
                                            "value": {
                                                "kind": "end",
                                                "message": format!("Done — {} files", total)
                                            }
                                        }
                                    }),
                                ).await;

                                // ── 4. Refresh all editors ───────────────────
                                crate::util::file_store::send_refresh_all().await;

                                let err_count = errors.len();
                                let msg = if err_count == 0 {
                                    format!("Rescanned {} files", ok_count)
                                } else {
                                    format!(
                                        "Rescanned {} files ({} errors)\n{}",
                                        ok_count,
                                        err_count,
                                        errors.join("\n")
                                    )
                                };

                                info!("rescan: {}", msg);

                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(json!({
                                            "ok": err_count == 0,
                                            "message": msg,
                                            "errors": errors
                                        })),
                                        error: None,
                                    },
                                ).await;
                            }

                            MethodCall::BuildExecute(params) => {
                                let uri = &params.uri;

                                let has_jass =
                                    crate::lng::jass::build::has_build_setting(uri, "build-jass");
                                let has_as =
                                    crate::lng::jass::build::has_build_setting(uri, "build-as");

                                let result = if has_jass && has_as {
                                    let r1 = crate::lng::jass::build::build_jass(uri);
                                    let r2 = crate::lng::jass::build::build_as(uri);
                                    if r1.ok && r2.ok {
                                        crate::lng::jass::build::BuildResult {
                                            ok: true,
                                            path: format!("{}, {}", r1.path, r2.path),
                                            message: format!("JASS: {} | AS: {}", r1.message, r2.message),
                                        }
                                    } else if !r1.ok {
                                        r1
                                    } else {
                                        r2
                                    }
                                } else if has_as {
                                    crate::lng::jass::build::build_as(uri)
                                } else {
                                    crate::lng::jass::build::build_jass(uri)
                                };

                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(json!(result)),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::BuildHooks(params) => {
                                let uri = &params.uri;
                                let (before_cmd, after_cmd, cwd) =
                                    crate::lng::jass::build::resolve_hooks(uri);
                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(json!({
                                            "before_cmd": before_cmd,
                                            "after_cmd": after_cmd,
                                            "cwd": cwd
                                        })),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::CodeAction(params) => {
                                code_action_send(call.id, &params).await;
                            }

                            MethodCall::SignatureHelp(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                signature_help_send(call.id, uri, &params.position).await;
                            }

                            MethodCall::CodeLens(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                code_lens_send(call.id, uri).await;
                            }

                            MethodCall::PrepareCallHierarchy(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                crate::lsp::call_hierarchy::send::send_prepare(
                                    call.id, uri, &params.position,
                                ).await;
                            }

                            MethodCall::IncomingCalls(params) => {
                                crate::lsp::call_hierarchy::send::send_incoming(
                                    call.id, &params.item,
                                ).await;
                            }

                            MethodCall::OutgoingCalls(params) => {
                                crate::lsp::call_hierarchy::send::send_outgoing(
                                    call.id, &params.item,
                                ).await;
                            }

                            MethodCall::PrepareTypeHierarchy(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                crate::lsp::type_hierarchy::send::send_prepare(
                                    call.id, uri, &params.position,
                                ).await;
                            }

                            MethodCall::Supertypes(params) => {
                                crate::lsp::type_hierarchy::send::send_supertypes(
                                    call.id, &params.item,
                                ).await;
                            }

                            MethodCall::Subtypes(params) => {
                                crate::lsp::type_hierarchy::send::send_subtypes(
                                    call.id, &params.item,
                                ).await;
                            }

                            MethodCall::UjapiDownload(params) => {
                                let source_uri = params.uri.clone();
                                let result = tokio::task::spawn_blocking(move || {
                                    let dest = match crate::util::ujapi::resolve_ujapi_path(&params.uri, &params.path) {
                                        Some(p) => p,
                                        None => return json!({
                                            "ok": false,
                                            "message": crate::util::i18n::ujapi_cannot_resolve_download_path(&params.path)
                                        }),
                                    };
                                    match crate::util::ujapi::download_common_j(&dest) {
                                        Ok(rel) => json!({
                                            "ok": true,
                                            "message": crate::util::i18n::ujapi_downloaded(&rel.tag, &dest.display().to_string()),
                                            "tag": rel.tag,
                                            "path": dest.display().to_string()
                                        }),
                                        Err(e) => json!({
                                            "ok": false,
                                            "message": crate::util::i18n::ujapi_download_failed(&e.to_string())
                                        }),
                                    }
                                }).await.unwrap_or_else(|e| json!({
                                    "ok": false,
                                    "message": format!("Task error: {}", e)
                                }));

                                // After successful download, re-parse the source file
                                // so the ujapi diagnostic clears and symbols become available.
                                if result.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                                    if let Some(path_str) = result.get("path").and_then(|v| v.as_str()) {
                                        let dest_path = std::path::PathBuf::from(path_str);
                                        if let Ok(content) = std::fs::read_to_string(&dest_path) {
                                            if let Ok(dest_uri) = Url::from_file_path(&dest_path) {
                                                if let Err(e) = crate::util::open::open_by_uri(&dest_uri, &content).await {
                                                    error!("ujapi: open downloaded file: {}", e);
                                                }
                                            }
                                        }
                                    }
                                    // Re-parse the file containing the //import-ujapi! directive.
                                    if let Ok(content) = source_uri.to_file_path().and_then(|p| std::fs::read_to_string(&p).map_err(|_| ())) {
                                        if let Err(e) = crate::util::open::open_by_uri(&source_uri, &content).await {
                                            error!("ujapi: re-parse source: {}", e);
                                        }
                                    }
                                    crate::util::file_store::send_refresh_all().await;
                                }

                                send(
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(result),
                                        error: None,
                                    },
                                ).await;
                            }

                            MethodCall::MpqInfo(params) => {
                                mpq_info_send(call.id, &params.archive_path).await;
                            }

                            MethodCall::MpqList(params) => {
                                mpq_list_send(call.id, &params.archive_path).await;
                            }

                            MethodCall::MpqRead(params) => {
                                mpq_read_send(call.id, &params.archive_path, &params.file_path).await;
                            }

                            _ => {
                                error!("Unexpected method call: {:?}", other);
                            }
                        }
                    });
                }
                }
            },

            LspMessage::RequestMessage(msg) => {
                match msg.method.as_str() {
                    "shutdown" | "exit" => {
                        send(
                            &json!({
                                "jsonrpc": "2.0",
                                "id": msg.id,
                                "result": null
                            }),
                        )
                        .await;
                        break;
                    }
                    _ => {
                        error!("Unexpected request: {:?}", msg);
                    }
                }
            }

            // Responses to server-initiated requests (refresh, etc.) — ignore.
            LspMessage::ClientResponse(_) => {}
        }
    }

    std::process::exit(0);
}
