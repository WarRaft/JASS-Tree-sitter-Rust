pub(crate) mod lsp;
pub(crate) mod util;

pub(crate) mod lng;

use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{self, BufReader, Stdout};
use tokio::sync::Mutex;

use crate::lng::blp::send::send as blp_send;
use crate::lng::doo::send::send as doo_send;
use crate::lng::w3i::send::send as w3i_send;
use crate::lsp::cancel::CancelCheck;
use crate::lsp::code_action::send::send as code_action_send;
use crate::lsp::completion::lsp::CompletionOptions;
use crate::lsp::completion::send::send as completion_send;
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
use crate::util::file_store::{mark_parse_pending, mark_parse_done, wait_for_parse, FILE_STORE, LSP_WRITER};
use crate::util::uri_map::LNG_URI_MAP;
use log::{error, info};
use url::Url;

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
                                    code_action_provider: Some(true),
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
                                if let Err(e) = lng::jass::parse::parse_and_notify(&uri).await {
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
                                if let Err(e) = lng::ass::parse::parse_and_notify(&uri).await {
                                    error!("as parse: {}", e);
                                }
                                mark_parse_done(&uri, parse_gen);
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
                                    if let Err(e) = lng::jass::parse::parse_and_notify(&uri).await {
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
                                    if let Err(e) = lng::ass::parse::parse_and_notify(&uri).await {
                                        error!("as parse: {}", e);
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
                    tokio::spawn(async move {
                        match other {
                            MethodCall::BlpRender(param) => {
                                blp_send(&writer, call.id, &param.uri).await;
                            }

                            MethodCall::DooRender(param) => {
                                doo_send(&writer, call.id, &param.uri).await;
                            }

                            MethodCall::W3iRender(param) => {
                                w3i_send(&writer, call.id, &param.uri).await;
                            }

                            MethodCall::Initialized(_) => {
                                use crate::util::file_cache;
                                use crate::util::import_graph::IMPORT_GRAPH;
                                use crate::util::scope_resolver::SCOPE_RESOLVER;
                                use std::collections::HashSet;

                                // ── 0. Force-load the scope resolver from disk ───────
                                let _ = SCOPE_RESOLVER.file_count();

                                // ── 0b. UjAPI release cache is now loaded lazily ─
                                // (triggered only when //import-ujapi! is encountered)

                                // ── 1. Load ALL cached data from unified disk cache ──
                                let cached_entries = file_cache::load_all();
                                let mut stale_uris: Vec<url::Url> = Vec::new();
                                let mut fresh_count = 0usize;

                                for (uri, cached) in &cached_entries {
                                    let current_meta = file_cache::FileMeta::from_uri(uri);
                                    if current_meta == Some(cached.meta) {
                                        // Fresh — reconstruct partial snapshot.
                                        let snapshot = std::sync::Arc::new(
                                            crate::util::file_store::ParseSnapshot {
                                                folding: Vec::new(),
                                                symbols: Vec::new(),
                                                semantic: std::sync::RwLock::new(Default::default()),
                                                diagnostics: Vec::new(),
                                                links: Vec::new(),
                                                ref_map: crate::lsp::ref_map::RefMap {
                                                    groups: cached.ref_map.groups.clone(),
                                                    spans: cached.ref_map.spans.clone(),
                                                    external_decls: cached.ref_map.external_decls.clone(),
                                                },
                                                file_symbols: cached.symbols.clone(),
                                                _type_map: Default::default(),
                                                type_hints: Vec::new(),
                                                ujapi_hints: Vec::new(),
                                                func_decl_keys: cached.func_decl_keys.clone(),
                                            },
                                        );
                                        FILE_STORE.insert(uri.clone(), snapshot);
                                        fresh_count += 1;
                                    } else if current_meta.is_some() {
                                        // File exists but changed — needs re-parse.
                                        stale_uris.push(uri.clone());
                                    }
                                    // If file doesn't exist anymore → skip (GC will clean).
                                }

                                info!(
                                    "file_cache: loaded {} fresh, {} stale",
                                    fresh_count,
                                    stale_uris.len()
                                );

                                // ── 2. GC orphaned graph nodes + caches ─────────────
                                let gc_removed = IMPORT_GRAPH.gc_orphans();
                                for orphan_uri in &gc_removed {
                                    SCOPE_RESOLVER.remove_file(orphan_uri);
                                }
                                let all = IMPORT_GRAPH.all_uris();
                                let keep: HashSet<String> =
                                    all.iter().map(|u| u.as_str().to_string()).collect();
                                let keep_urls: HashSet<url::Url> =
                                    all.iter().cloned().collect();
                                file_cache::gc(&keep);
                                SCOPE_RESOLVER.gc(&keep_urls);

                                // ── 3. Re-parse stale files with progress ────────────
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
                                                if let Err(e) = crate::util::open::open_by_uri(uri, &content).await {
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

                                // ── 4. Register file watchers ─────────────────────────
                                // VS Code only sends textDocument/didChange for files
                                // open in the editor.  To detect external changes (file
                                // created / modified / deleted on disk) we register
                                // workspace/didChangeWatchedFiles watchers.
                                {
                                    use std::sync::atomic::{AtomicI64, Ordering};
                                    static REG_ID: AtomicI64 = AtomicI64::new(-1000);
                                    let id = REG_ID.fetch_sub(1, Ordering::Relaxed);
                                    send(
                                        &writer,
                                        &json!({
                                            "jsonrpc": "2.0",
                                            "id": id,
                                            "method": "client/registerCapability",
                                            "params": {
                                                "registrations": [{
                                                    "id": "file-watcher-j",
                                                    "method": "workspace/didChangeWatchedFiles",
                                                    "registerOptions": {
                                                        "watchers": [
                                                            { "globPattern": "**/*.j",  "kind": 7 },
                                                            { "globPattern": "**/*.as", "kind": 7 }
                                                        ]
                                                    }
                                                }]
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
                                wait_for_parse(uri, Duration::from_secs(5)).await;
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                semantic_send(&writer, call.id, uri, None).await
                            }

                            MethodCall::SemanticRange(params) => {
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                wait_for_parse(uri, Duration::from_secs(5)).await;
                                if call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                semantic_send(&writer, call.id, uri, Some(params.range)).await
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

                            MethodCall::RescanExecute(_params) => {
                                use crate::util::file_cache;
                                use crate::util::import_graph::IMPORT_GRAPH;
                                use crate::util::scope_resolver::SCOPE_RESOLVER;

                                info!("rescan: starting forced full rescan");

                                // ── 1. GC orphan nodes, then collect all known URIs ──
                                let gc_removed = IMPORT_GRAPH.gc_orphans();
                                for orphan_uri in &gc_removed {
                                    SCOPE_RESOLVER.remove_file(orphan_uri);
                                }
                                let all_uris = IMPORT_GRAPH.all_uris();
                                let total = all_uris.len();

                                if total == 0 {
                                    send(
                                        &writer,
                                        &ResponseMessage {
                                            jsonrpc: "2.0".into(),
                                            id: call.id,
                                            result: Some(json!({
                                                "ok": false,
                                                "message": "No files in import graph"
                                            })),
                                            error: None,
                                        },
                                    ).await;
                                    return;
                                }

                                // ── 2. Purge ALL caches ──────────────────────────────
                                file_cache::purge_all();
                                SCOPE_RESOLVER.clear_all();
                                FILE_STORE.clear();

                                // ── 3. Re-parse every file with progress ────────────
                                let token = "jass-rescan-full";

                                send(
                                    &writer,
                                    &json!({
                                        "jsonrpc": "2.0",
                                        "id": 99998,
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
                                                "title": "JASS: Full rescan",
                                                "cancellable": false,
                                                "percentage": 0
                                            }
                                        }
                                    }),
                                ).await;

                                let mut ok_count = 0usize;
                                let mut errors: Vec<String> = Vec::new();

                                for (i, uri) in all_uris.iter().enumerate() {
                                    let fname = uri.path().rsplit('/').next().unwrap_or("");
                                    let pct = ((i + 1) * 100 / total) as u32;
                                    info!("rescan {}/{} {}", i + 1, total, fname);

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

                                    match uri.to_file_path() {
                                        Ok(path) => match std::fs::read_to_string(&path) {
                                            Ok(content) => {
                                                if let Err(e) = crate::util::open::open_by_uri(uri, &content).await {
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
                                    &writer,
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

                                // ── 4. Refresh all editors ───────────────────────────
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
                                    &writer,
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

                            MethodCall::CodeAction(params) => {
                                code_action_send(&writer, call.id, &params).await;
                            }

                            MethodCall::UjapiDownload(params) => {
                                let source_uri = params.uri.clone();
                                let result = tokio::task::spawn_blocking(move || {
                                    let dest = match crate::util::ujapi::resolve_ujapi_path(&params.uri, &params.path) {
                                        Some(p) => p,
                                        None => return json!({
                                            "ok": false,
                                            "message": format!("Cannot resolve path: {}", params.path)
                                        }),
                                    };
                                    match crate::util::ujapi::download_common_j(&dest) {
                                        Ok(rel) => json!({
                                            "ok": true,
                                            "message": format!("Downloaded UjAPI {} to {}", rel.tag, dest.display()),
                                            "tag": rel.tag,
                                            "path": dest.display().to_string()
                                        }),
                                        Err(e) => json!({
                                            "ok": false,
                                            "message": format!("Download failed: {}", e)
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
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(result),
                                        error: None,
                                    },
                                ).await;
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
