//! WebSocket handlers for debug log, rescan, and diagnostics streaming.
//!
//! - `GET /ws/log?token=...` — debug log stream
//! - `GET /ws/rescan?token=...` — rescan progress (send `{"uri":"..."}` as first frame)
//! - `GET /ws/diagnostics?token=...` — diagnostics progress (send `{"uri":"..."}` as first frame)

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::Query;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;

use crate::http::server::{TokenParam, check_token};
use crate::http::api::AuthQuery;

// ─── Debug log ───────────────────────────────────────────────────────────────

pub async fn ws_debug_log(
    ws: WebSocketUpgrade,
    Query(auth): Query<AuthQuery>,
) -> Result<impl IntoResponse, (axum::http::StatusCode, &'static str)> {
    check_token(&TokenParam { token: auth.token })?;
    Ok(ws.on_upgrade(handle_debug_log))
}

async fn handle_debug_log(socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let recent = crate::util::debug_log::recent();

    // Send initial greeting so the client knows the connection is alive
    let port = crate::http::server::BINARY_SERVER
        .get()
        .map(|i| i.port)
        .unwrap_or(0);
    let _ = sink
        .send(Message::Text(format!("server connected on port {port}").into()))
        .await;

    for msg in recent {
        if sink.send(Message::Text(msg.into())).await.is_err() {
            return;
        }
    }

    let mut rx = crate::util::debug_log::subscribe();

    // Forward broadcast messages → WS text frames
    let send_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    if sink.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    let _ = sink.send(Message::Text(format!("[LAGGED: {} messages lost]", n).into())).await;
                    continue;
                }
                Err(_) => break,
            }
        }
    });

    // Drain incoming frames (just pongs / close); ignore content
    while let Some(Ok(msg)) = stream.next().await {
        if matches!(msg, Message::Close(_)) {
            break;
        }
    }

    send_task.abort();
}

// ─── Rescan ──────────────────────────────────────────────────────────────────

pub async fn ws_rescan(
    ws: WebSocketUpgrade,
    Query(auth): Query<AuthQuery>,
) -> Result<impl IntoResponse, (axum::http::StatusCode, &'static str)> {
    check_token(&TokenParam { token: auth.token })?;
    Ok(ws.on_upgrade(handle_rescan))
}

type WsSink = futures_util::stream::SplitSink<WebSocket, Message>;

enum RescanSourceRead {
    SkipDir,
    Ready { rel: String, content: String },
    Error(String),
}

struct RescanRunResult {
    errors: Vec<String>,
    scanned_files: Vec<String>,
    ok_count: usize,
}

fn compute_common_parent(tree_list: &[url::Url]) -> std::path::PathBuf {
    let abs_paths: Vec<std::path::PathBuf> = tree_list
        .iter()
        .filter_map(|u| u.to_file_path().ok())
        .collect();

    if abs_paths.is_empty() {
        return std::path::PathBuf::new();
    }

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
}

fn read_rescan_source(uri: &url::Url, common_parent: &std::path::Path) -> RescanSourceRead {
    match uri.to_file_path() {
        Ok(path) if path.is_dir() => RescanSourceRead::SkipDir,
        Ok(path) => {
            let rel = path
                .strip_prefix(common_parent)
                .unwrap_or(&path)
                .display()
                .to_string();
            match std::fs::read_to_string(&path) {
                Ok(content) => RescanSourceRead::Ready { rel, content },
                Err(e) => RescanSourceRead::Error(format!("cannot read — {e}")),
            }
        }
        Err(_) => RescanSourceRead::Error("invalid file path".to_string()),
    }
}

fn preload_rescan_uri(uri: &url::Url, content: &str) -> Result<(), String> {
    crate::util::open::preload_symbols_by_uri(uri, content)
        .map_err(|e| format!("init — {e}"))
}

async fn parse_rescan_uri(uri: &url::Url, content: &str) -> Result<(), String> {
    crate::util::open::parse_only_by_uri(uri, content)
        .await
        .map_err(|e| format!("parse — {e}"))
}

async fn send_rescan_step_progress(
    sink: &mut WsSink,
    step: u8,
    file: &str,
    index: usize,
    total: usize,
) {
    let progress = json!({
        "step": step,
        "file": file,
        "index": index,
        "total": total,
    });
    let _ = sink.send(Message::Text(progress.to_string().into())).await;
}

fn build_rescan_uri_entries(tree_list: &[url::Url]) -> Vec<serde_json::Value> {
    tree_list
        .iter()
        .map(|u| {
            let lang = if crate::util::open::is_as_uri(u) {
                "angelscript"
            } else {
                "jass"
            };
            json!({"uri": u.to_string(), "languageId": lang})
        })
        .collect()
}

async fn run_rescan_passes(
    sink: &mut WsSink,
    tree_list: &[url::Url],
    common_parent: &std::path::Path,
) -> RescanRunResult {
    let total = tree_list.len();
    let mut errors: Vec<String> = Vec::new();
    let mut scanned_files: Vec<String> = Vec::new();
    let mut contents: Vec<Option<String>> = Vec::with_capacity(tree_list.len());

    for (index, u) in tree_list.iter().enumerate() {
        let fname = u.path().rsplit('/').next().unwrap_or("");
        send_rescan_step_progress(sink, 1, fname, index, total).await;

        match read_rescan_source(u, common_parent) {
            RescanSourceRead::SkipDir => {
                contents.push(None);
            }
            RescanSourceRead::Error(err) => {
                errors.push(format!("{}: {}", fname, err));
                contents.push(None);
            }
            RescanSourceRead::Ready { rel, content } => {
                scanned_files.push(rel);
                if let Err(err) = preload_rescan_uri(u, &content) {
                    errors.push(format!("{}: {}", fname, err));
                    contents.push(None);
                    continue;
                }
                contents.push(Some(content));
            }
        }
    }

    let mut ok_count = 0usize;
    for (index, u) in tree_list.iter().enumerate() {
        let content = match contents.get(index).and_then(|c| c.as_deref()) {
            Some(c) => c,
            None => continue,
        };
        let fname = u.path().rsplit('/').next().unwrap_or("");
        send_rescan_step_progress(sink, 2, fname, index, total).await;

        if let Err(err) = parse_rescan_uri(u, content).await {
            errors.push(format!("{}: {}", fname, err));
        } else {
            ok_count += 1;
        }
    }

    RescanRunResult {
        errors,
        scanned_files,
        ok_count,
    }
}

async fn handle_rescan(socket: WebSocket) {
    use crate::util::import_graph::IMPORT_GRAPH;
    use crate::util::parse_cache::PARSE_CACHE;
    use crate::util::rescan::RescanGuard;
    use url::Url;

    let (mut sink, mut stream) = socket.split();

    // Wait for the first message containing {"uri":"..."}
    let uri: Url = loop {
        match stream.next().await {
            Some(Ok(Message::Text(text))) => {
                match serde_json::from_str::<crate::http::api::UriParam>(&text) {
                    Ok(p) => break p.uri,
                    Err(e) => {
                        let _ = sink.send(Message::Text(
                            json!({"error": format!("bad payload: {e}")}).to_string().into()
                        )).await;
                        let _ = sink.close().await;
                        return;
                    }
                }
            }
            Some(Ok(Message::Close(_))) | None => return,
            _ => continue,
        }
    };

    // Only one rescan at a time
    let guard = match RescanGuard::try_acquire() {
        Some(g) => g,
        None => {
            let _ = sink.send(Message::Text(
                json!({"busy": true, "message": "Rescan already in progress"}).to_string().into()
            )).await;
            let _ = sink.close().await;
            return;
        }
    };

    let tree_uris = IMPORT_GRAPH.tree_for_uri(&uri);
    if tree_uris.is_empty() {
        drop(guard);
        let _ = sink.send(Message::Text(
            json!({"ok": false, "message": "No files in tree"}).to_string().into()
        )).await;
        let _ = sink.close().await;
        return;
    }

    let tree_list: Vec<Url> = tree_uris.iter().cloned().collect();

    // Purge caches before the scan loop.
    crate::util::file_cache::purge_set(&tree_uris);
    for u in &tree_list { PARSE_CACHE.remove(u); }

    let _guard = guard;

    let common_parent = compute_common_parent(&tree_list);

    let run = run_rescan_passes(&mut sink, &tree_list, &common_parent).await;
    let uri_entries = build_rescan_uri_entries(&tree_list);

    let msg = if run.errors.is_empty() {
        format!("Rescanned {} files", run.ok_count)
    } else {
        format!(
            "Rescanned {} files ({} errors)\n{}",
            run.ok_count,
            run.errors.len(),
            run.errors.join("\n")
        )
    };

    let done = json!({
        "done": true,
        "ok": run.errors.is_empty(),
        "message": msg,
        "errors": run.errors,
        "files": run.scanned_files,
        "entries": uri_entries,
        "root": common_parent.display().to_string(),
    });
    let _ = sink.send(Message::Text(done.to_string().into())).await;
    let _ = sink.close().await;
}

// ─── Diagnostics ─────────────────────────────────────────────────────────────

pub async fn ws_diagnostics(
    ws: WebSocketUpgrade,
    Query(auth): Query<AuthQuery>,
) -> Result<impl IntoResponse, (axum::http::StatusCode, &'static str)> {
    check_token(&TokenParam { token: auth.token })?;
    Ok(ws.on_upgrade(handle_diagnostics))
}

async fn handle_diagnostics(socket: WebSocket) {
    use crate::util::import_graph::IMPORT_GRAPH;
    use crate::util::parse_cache::PARSE_CACHE;

    let (mut sink, mut stream) = socket.split();

    // Wait for the first message containing {"uri":"..."}
    let uri: url::Url = loop {
        match stream.next().await {
            Some(Ok(Message::Text(text))) => {
                match serde_json::from_str::<crate::http::api::DiagnosticsParam>(&text) {
                    Ok(p) => break p.uri,
                    Err(e) => {
                        let _ = sink.send(Message::Text(
                            json!({"error": format!("bad payload: {e}")}).to_string().into()
                        )).await;
                        let _ = sink.close().await;
                        return;
                    }
                }
            }
            Some(Ok(Message::Close(_))) | None => return,
            _ => continue,
        }
    };

    let tree_list = IMPORT_GRAPH.tree_for_uri_sorted(&uri);
    if tree_list.is_empty() {
        let _ = sink.send(Message::Text(
            json!({"done": true, "files": []}).to_string().into()
        )).await;
        let _ = sink.close().await;
        return;
    }

    let total = tree_list.len();
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
        let _ = sink.send(Message::Text(progress.to_string().into())).await;

        let (errors, warnings, hints, info_count) = if let Some(snap) = PARSE_CACHE.get(peer) {
            crate::http::api::count_snap_diagnostics(&snap)
        } else {
            let peer_clone = peer.clone();
            tokio::task::spawn_blocking(move || crate::http::api::count_diagnostics_isolated(&peer_clone))
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
        let _ = sink.send(Message::Text(file_event.to_string().into())).await;

        files.push(file_entry);
    }

    let done = json!({
        "done": true,
        "files": files,
    });
    let _ = sink.send(Message::Text(done.to_string().into())).await;
    let _ = sink.close().await;
}
