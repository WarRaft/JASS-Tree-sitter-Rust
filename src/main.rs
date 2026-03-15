pub(crate) mod lsp;
pub(crate) mod util;

pub(crate) mod lng;

use serde_json::json;
use std::sync::Arc;
use tokio::io::{self, BufReader, Stdout};
use tokio::sync::Mutex;

use crate::lng::blp::send::send as blp_send;
use crate::lsp::cancel::CancelCheck;
use crate::lsp::completion::lsp::CompletionOptions;
use crate::lsp::completion::send::send as completion_send;
use crate::lsp::diagnostic::lsp::DiagnosticOptions;
use crate::lsp::document_link::lsp::DocumentLinkOptions;
use crate::lsp::document_symbol::lsp::DocumentSymbolOptions;
use crate::lsp::folding::lsp::FoldingRangeOptions;
use crate::lsp::formatting::lsp::DocumentFormattingOptions;
use crate::lsp::formatting::send::send_formatting;
use crate::lsp::highlight::send::send as highlight_send;
use crate::lsp::hover::send::send as hover_send;
use crate::lsp::inlay_hint::send::send as inlay_hint_send;
use crate::lsp::initialize::{InitializeResult, ServerCapabilities};
use crate::lsp::protocol::{LspMessage, MethodCall, ResponseMessage};
use crate::lsp::read::read;
use crate::lsp::rename::handle::compute_rename_edits;
use crate::lsp::rename::identifier::{compute_identifier_rename, prepare_rename};
use crate::lsp::rename::lsp::{
    FileOperationFilter, FileOperationOptions, FileOperationPattern,
    FileOperationRegistrationOptions, WorkspaceServerCapabilities,
};
use crate::lsp::semantic::lsp::{
    Kind, Mod, SemanticTokensFullOptions, SemanticTokensFullOptionsObject, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensRangeProviderCapability, ToCamelVec,
};
use crate::lsp::semantic::send::send as semantic_send;
use crate::lsp::send::send;
use crate::lsp::send::send_cancelled;
use crate::lsp::text_document::{TextDocumentSyncKind, TextDocumentSyncOptions};
use crate::util::file_store::{diagnostic_report, FILE_STORE, LSP_WRITER};
use crate::util::uri_map::LNG_URI_MAP;
use log::{error, info};

#[tokio::main]
async fn main() {
    env_logger::init();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin);
    let writer = Arc::new(Mutex::new(stdout));
    // Set the global writer for background push notifications.
    let _ = LSP_WRITER.set(writer.clone());

    loop {
        let msg = match read(&mut reader).await {
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

        let writer: Arc<Mutex<Stdout>> = writer.clone();

        match parsed {
            LspMessage::Call(call) => match call.payload {
                MethodCall::Initialize(_) => {
                    send(
                        &writer,
                        &ResponseMessage {
                            jsonrpc: "2.0".into(),
                            id: call.id,
                            result: Some(InitializeResult {
                                capabilities: ServerCapabilities {
                                    text_document_sync: Some(TextDocumentSyncOptions {
                                        open_close: Some(true),
                                        change: Some(TextDocumentSyncKind::Incremental),
                                    }),
                                    semantic_tokens_provider: Some(SemanticTokensOptions {
                                        legend: SemanticTokensLegend {
                                            token_types: <Kind as ToCamelVec>::get_vec(),
                                            token_modifiers: <Mod as ToCamelVec>::get_vec(),
                                        },
                                        range: Some(SemanticTokensRangeProviderCapability::Simple(
                                            true,
                                        )),
                                        full: Some(SemanticTokensFullOptions::Options(
                                            SemanticTokensFullOptionsObject { delta: Some(false) },
                                        )),
                                    }),
                                    diagnostic_provider: Some(DiagnosticOptions {
                                        inter_file_dependencies: true,
                                        workspace_diagnostics: false,
                                        ..Default::default()
                                    }),
                                    document_symbol_provider: Some(DocumentSymbolOptions {
                                        label: None,
                                    }),
                                    folding_range_provider: Some(FoldingRangeOptions {}),
                                    completion_provider: Some(CompletionOptions {
                                        trigger_characters: Some(vec![
                                            "/".into(),
                                            "\\".into(),
                                        ]),
                                    }),
                                    hover_provider: Some(true),
                                    document_highlight_provider: Some(true),
                                    definition_provider: Some(true),
                                    references_provider: Some(true),
                                    inlay_hint_provider: Some(
                                        crate::lsp::inlay_hint::lsp::InlayHintOptions {
                                            resolve_provider: None,
                                        },
                                    ),
                                    rename_provider: Some(
                                        crate::lsp::rename::lsp::RenameOptions {
                                            prepare_provider: Some(true),
                                        },
                                    ),
                                    document_link_provider: Some(DocumentLinkOptions {
                                        resolve_provider: Some(false),
                                    }),
                                    document_formatting_provider: Some(
                                        DocumentFormattingOptions {},
                                    ),
                                    workspace: Some(WorkspaceServerCapabilities {
                                        file_operations: Some(FileOperationOptions {
                                            will_rename: Some(FileOperationRegistrationOptions {
                                                filters: vec![FileOperationFilter {
                                                    scheme: Some("file".into()),
                                                    pattern: FileOperationPattern {
                                                        glob: "**/*".into(),
                                                        matches: None,
                                                    },
                                                }],
                                            }),
                                        }),
                                    }),
                                    ..Default::default()
                                },
                            }),
                            error: None,
                        },
                    )
                    .await
                }

                MethodCall::Shutdown() | MethodCall::Exit() => {
                    send(
                        &writer,
                        &ResponseMessage {
                            jsonrpc: "2.0".into(),
                            id: None,
                            result: Some(json!(null)),
                            error: None,
                        },
                    )
                    .await;
                    break;
                }

                // ─── Notifications processed inline to preserve ordering ─────

                MethodCall::Cancel(params) => {
                    params.id.mark_cancelled().await;
                }

                MethodCall::SetTrace(_) => {}

                MethodCall::DidClose(_) => {}

                MethodCall::DidOpen(params) => {
                    if params.text_document.language_id == "bni" {
                        let text = params.text_document.text;
                        let uri = params.text_document.uri;
                        if let Err(e) = lng::bni::open::init(&uri, &text) {
                            error!("bni init: {}", e);
                        } else {
                            tokio::spawn(async move {
                                if let Err(e) = lng::bni::parse::parse_and_notify(&uri).await {
                                    error!("bni parse: {}", e);
                                }
                            });
                        }
                    } else if params.text_document.language_id == "jass" {
                        let text = params.text_document.text;
                        let uri = params.text_document.uri;
                        if let Err(e) = lng::jass::open::init(&uri, &text) {
                            error!("jass init: {}", e);
                        } else {
                            tokio::spawn(async move {
                                if let Err(e) = lng::jass::parse::parse_and_notify(&uri).await {
                                    error!("jass parse: {}", e);
                                }
                            });
                        }
                    } else if params.text_document.language_id == "angelscript" {
                        let text = params.text_document.text;
                        let uri = params.text_document.uri;
                        if let Err(e) = lng::ass::open::init(&uri, &text) {
                            error!("as init: {}", e);
                        } else {
                            tokio::spawn(async move {
                                if let Err(e) = lng::ass::parse::parse_and_notify(&uri).await {
                                    error!("as parse: {}", e);
                                }
                            });
                        }
                    }
                }

                MethodCall::DidChange(params) => {
                    let uri = params.text_document.uri;

                    if let Some(lng) = LNG_URI_MAP.get(&uri) {
                        let lng_val = lng.value().clone();
                        drop(lng); // release DashMap guard before calling apply_edits

                        if lng_val == "bni" {
                            if let Err(e) = lng::bni::change::apply_edits(&uri, params.content_changes) {
                                error!("bni edit: {}", e);
                            } else {
                                tokio::spawn(async move {
                                    if let Err(e) = lng::bni::parse::parse_and_notify(&uri).await {
                                        error!("bni parse: {}", e);
                                    }
                                });
                            }
                        } else if lng_val == "jass" {
                            if let Err(e) = lng::jass::change::apply_edits(&uri, params.content_changes) {
                                error!("jass edit: {}", e);
                            } else {
                                tokio::spawn(async move {
                                    if let Err(e) = lng::jass::parse::parse_and_notify(&uri).await {
                                        error!("jass parse: {}", e);
                                    }
                                });
                            }
                        } else if lng_val == "angelscript" {
                            if let Err(e) = lng::ass::change::apply_edits(&uri, params.content_changes) {
                                error!("as edit: {}", e);
                            } else {
                                tokio::spawn(async move {
                                    if let Err(e) = lng::ass::parse::parse_and_notify(&uri).await {
                                        error!("as parse: {}", e);
                                    }
                                });
                            }
                        }
                    }
                }

                // ─── All other methods spawned as concurrent request handlers ─

                other => {
                    tokio::spawn(async move {
                        match other {
                            MethodCall::BlpRender(param) => {
                                blp_send(&writer, call.id, &param.uri).await;
                            }

                            MethodCall::Initialized(_) => {
                                use crate::lng::jass::symbol::FILE_SYMBOLS;
                                use crate::util::import_graph::IMPORT_GRAPH;
                                use crate::util::ref_cache;
                                use crate::util::symbol_cache;
                                use crate::util::scope_resolver::SCOPE_RESOLVER;
                                use crate::lsp::ref_map::REF_URI_MAP;
                                use std::collections::HashSet;

                                // ── 0. Force-load the scope resolver from disk ───────
                                let _ = SCOPE_RESOLVER.file_count();

                                // ── 1. Load ALL cached FileSymbols from disk ─────────
                                let cached_entries = symbol_cache::load_all();
                                let mut stale_uris: Vec<url::Url> = Vec::new();

                                for (uri, cached_meta, symbols) in &cached_entries {
                                    let current_meta = symbol_cache::FileMeta::from_uri(uri);
                                    if current_meta == Some(*cached_meta) {
                                        // Fresh — load into memory immediately.
                                        FILE_SYMBOLS.insert(uri.clone(), symbols.clone());
                                    } else if current_meta.is_some() {
                                        // File exists but changed — needs re-parse.
                                        stale_uris.push(uri.clone());
                                    }
                                    // If file doesn't exist anymore → skip (GC will clean).
                                }

                                info!(
                                    "symbol_cache: loaded {} fresh, {} stale",
                                    cached_entries.len() - stale_uris.len(),
                                    stale_uris.len()
                                );

                                // ── 2. GC orphaned caches ────────────────────────────
                                let all = IMPORT_GRAPH.all_uris();
                                let keep: HashSet<String> =
                                    all.iter().map(|u| u.as_str().to_string()).collect();
                                let keep_urls: HashSet<url::Url> =
                                    all.iter().cloned().collect();
                                ref_cache::gc(&keep);
                                symbol_cache::gc(&keep);
                                SCOPE_RESOLVER.gc(&keep_urls);

                                // ── 3. Load cached RefMaps for fresh files ───────────
                                for uri in &all {
                                    if REF_URI_MAP.contains_key(uri) {
                                        continue;
                                    }
                                    if let Ok(path) = uri.to_file_path() {
                                        if path.exists() {
                                            if let Ok(content) = std::fs::read_to_string(&path) {
                                                let rope = lapce_xi_rope::Rope::from(content.as_str());
                                                let hash = ref_cache::content_hash(&rope);
                                                if let Some(ref_map) = ref_cache::load(uri, &hash) {
                                                    REF_URI_MAP.insert(uri.clone(), ref_map);
                                                }
                                            }
                                        }
                                    }
                                }

                                // ── 4. Re-parse stale files with progress ────────────
                                if !stale_uris.is_empty() {
                                    let total = stale_uris.len();
                                    let token = "jass-rescan";

                                    send(
                                        &writer,
                                        &json!({
                                            "jsonrpc": "2.0",
                                            "id": 99999,
                                            "method": "window/workDoneProgress/create",
                                            "params": { "token": token }
                                        }),
                                    ).await;

                                    send(
                                        &writer,
                                        &json!({
                                            "jsonrpc": "2.0",
                                            "method": "$/progress",
                                            "params": {
                                                "token": token,
                                                "value": {
                                                    "kind": "begin",
                                                    "title": "JASS: Rescanning files",
                                                    "cancellable": false,
                                                    "percentage": 0
                                                }
                                            }
                                        }),
                                    ).await;

                                    for (i, uri) in stale_uris.iter().enumerate() {
                                        let pct = ((i + 1) * 100 / total) as u32;
                                        let path_str = uri.path();
                                        let fname = path_str.rsplit('/').next().unwrap_or("");
                                        send(
                                            &writer,
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

                                        if let Ok(path) = uri.to_file_path() {
                                            if let Ok(content) = std::fs::read_to_string(&path) {
                                                if let Err(e) = lng::jass::open::open(uri, &content).await {
                                                    error!("rescan {}: {}", uri, e);
                                                }
                                            }
                                        }
                                    }

                                    send(
                                        &writer,
                                        &json!({
                                            "jsonrpc": "2.0",
                                            "method": "$/progress",
                                            "params": {
                                                "token": token,
                                                "value": {
                                                    "kind": "end",
                                                    "message": format!("Done — {} files rescanned", total)
                                                }
                                            }
                                        }),
                                    ).await;
                                }
                            }

                            MethodCall::SemanticFull(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                semantic_send(&writer, call.id, uri, None).await
                            }

                            MethodCall::SemanticRange(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                semantic_send(&writer, call.id, uri, Some(params.range)).await
                            }

                            MethodCall::Diagnostic(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }

                                let uri = &params.text_document.uri;
                                let result = diagnostic_report(uri);

                                send(
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(&result),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::DocumentSymbol(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }

                                let uri = &params.text_document.uri;
                                let snapshot = FILE_STORE.get(uri);
                                let result: Option<&Vec<_>> =
                                    snapshot.as_ref().map(|s| &s.value().symbols);

                                send(
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result,
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::Folding(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;

                                let snapshot = FILE_STORE.get(uri);
                                let empty_vec = vec![];
                                let result: &Vec<_> = snapshot
                                    .as_ref()
                                    .map(|s| &s.value().folding)
                                    .unwrap_or(&empty_vec);

                                send(
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(result),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::Completion(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                completion_send(&writer, call.id, uri, &params.position).await;
                            }

                            MethodCall::Hover(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                hover_send(&writer, call.id, uri, &params.position).await;
                            }

                            MethodCall::DocumentHighlight(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                highlight_send(&writer, call.id, &params).await;
                            }

                            MethodCall::Definition(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;

                                let result = {
                                    use crate::lsp::ref_map::REF_URI_MAP;
                                    let mut locs = Vec::new();
                                    if let Some(snapshot) = FILE_STORE.get(uri) {
                                        if let Some(rope_entry) = crate::util::roper::uri_map::ROPE_MAP.get(uri) {
                                            if let Some(byte) = params.position.to_byte_offset(rope_entry.value()) {
                                                let ref_map = &snapshot.ref_map;
                                                if let Some(ext) = ref_map.external_at(byte) {
                                                    if let Some(ext_ref_entry) = REF_URI_MAP.get(&ext.uri) {
                                                        let ext_ref_map = ext_ref_entry.value();
                                                        for group in ext_ref_map.groups.values() {
                                                            if group.name == ext.name {
                                                                for occ in &group.occurrences {
                                                                    if occ.is_decl {
                                                                        locs.push(crate::lsp::location::Location {
                                                                            uri: ext.uri.to_string(),
                                                                            range: occ.range.clone(),
                                                                        });
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
                                    &writer,
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
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
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
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(&result),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::InlayHint(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                inlay_hint_send(&writer, call.id, &params).await;
                            }

                            MethodCall::DocumentLink(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;

                                let snapshot = FILE_STORE.get(uri);
                                let empty_vec = vec![];
                                let result: &Vec<crate::lsp::document_link::lsp::DocumentLink> =
                                    snapshot
                                        .as_ref()
                                        .map(|s| &s.value().links)
                                        .unwrap_or(&empty_vec);

                                send(
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(result),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::Formatting(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                send_formatting(&writer, call.id, &params).await;
                            }

                            MethodCall::PrepareRename(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;

                                let result = prepare_rename(uri, &params.position);
                                send(
                                    &writer,
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
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;

                                let edit = compute_identifier_rename(
                                    uri,
                                    &params.position,
                                    &params.new_name,
                                );
                                send(
                                    &writer,
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
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(edit),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::ImportGraphSubgraph(params) => {
                                let uri = &params.uri;
                                let (nodes, edges) =
                                    crate::util::import_graph::IMPORT_GRAPH
                                        .subgraph_for(uri);
                                send(
                                    &writer,
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
                                    &writer,
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
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(json!(result)),
                                        error: None,
                                    },
                                )
                                .await;
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
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(json!(result)),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            _ => {
                                error!("Unexpected method call: {:?}", other);
                            }
                        }
                    });
                }
            },

            LspMessage::RequestMessage(msg) => {
                match msg.method.as_str() {
                    "shutdown" | "exit" => {
                        send(
                            &writer,
                            &ResponseMessage {
                                jsonrpc: "2.0".into(),
                                id: None,
                                result: Some(json!(null)),
                                error: None,
                            },
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
