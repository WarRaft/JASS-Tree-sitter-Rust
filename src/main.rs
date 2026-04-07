pub(crate) mod http;
pub(crate) mod lsp;
pub(crate) mod util;

pub(crate) mod lng;

use serde_json::json;
use tokio::sync::mpsc;

use crate::lsp::protocol::WsNotification;
use crate::util::file_store::{
    mark_parse_pending, mark_parse_done, FILE_STORE,
};
use crate::util::uri_map::LNG_URI_MAP;
use log::error;

#[tokio::main]
async fn main() {
    env_logger::init();

    // ── Start HTTP + WebSocket server ────────────────────────────
    let http_port = http::server::start_server().await.ok();

    // ── Print port + token to stdout so the extension can connect ─
    if let (Some(port), Some(info)) = (http_port, http::server::BINARY_SERVER.get()) {
        let startup = json!({
            "port": port,
            "token": &info.token,
        });
        println!("{}", startup);
        use std::io::Write;
        std::io::stdout().flush().ok();
    }

    // ── Eagerly build snapshot if game path already configured ──
    tokio::task::spawn_blocking(|| {
        let gp = lng::w3e::game_path::get_game_path();
        if !gp.is_empty() {
            log::info!("Game path found on startup, building snapshot…");
            lng::w3e::snapshot::build_snapshot(None);
        }
    });

    // ── Create channel for incoming WebSocket messages ───────────
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<String>();
    let _ = http::ws::MSG_TX.set(msg_tx);

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
                Ok(_) => {}
            }
        }
    });

    // ── Main dispatch loop — only notifications from WebSocket ────
    loop {
        let msg = match msg_rx.recv().await {
            Some(msg) => msg,
            None => break,
        };

        let notification = match serde_json::from_str::<WsNotification>(&msg) {
            Ok(n) => n,
            Err(err) => {
                error!("Failed to parse notification: {} |{}", err, msg);
                continue;
            }
        };

        match notification {
            WsNotification::DidClose(params) => {
                let uri = params.text_document.uri;

                let evicted = util::file_store::evict_closed_file(&uri);

                // Send empty parseResult for every evicted URI so the
                // extension clears stale markers, hints, etc.
                if !evicted.is_empty() {
                    tokio::spawn(async move {
                        for evicted_uri in &evicted {
                            lsp::send::send(
                                &json!({
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

            WsNotification::DidOpen(params) => {
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

            WsNotification::DidChange(params) => {
                let uri = params.text_document.uri;

                if let Some(lng) = LNG_URI_MAP.get(&uri) {
                    let lng_val = lng.value().clone();
                    drop(lng);

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

            WsNotification::DidChangeWatchedFiles(params) => {
                use crate::util::import_graph::IMPORT_GRAPH;

                let mut dependents_to_reparse: std::collections::HashSet<url::Url> =
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
                            if util::roper::uri_map::ROPE_MAP.contains_key(uri) {
                                continue;
                            }
                            if let Ok(path) = uri.to_file_path() {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    if let Err(e) = util::open::open_by_uri(uri, &content).await {
                                        error!("file-watcher reparse {}: {}", uri, e);
                                    }
                                }
                            }
                        }
                        util::file_store::send_refresh_all().await;
                    });
                }
            }
        }
    }

    std::process::exit(0);
}
