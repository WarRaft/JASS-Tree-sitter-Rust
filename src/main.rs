pub(crate) mod http;
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
use crate::lng::mdx::send::send as mdx_send;
use crate::lng::slk::send::send as slk_send;
use crate::lng::w3abdhqtu::send::send as w3obj_send;
use crate::lng::w3e::send::send as w3e_send;
use crate::lng::w3i::send::send as w3i_send;
use crate::lng::mpq::send::{send_info as mpq_info_send, send_list as mpq_list_send, send_read as mpq_read_send};
use crate::lsp::cancel::CancelCheck;
use crate::lsp::code_action::send::send as code_action_send;
use crate::lsp::code_lens::send::send as code_lens_send;
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
use crate::lsp::signature_help::send::send as signature_help_send;
use crate::lsp::text_document::{TextDocumentSyncKind, TextDocumentSyncOptions};
use crate::util::debug_log::{send_debug_log, DebugStatus, DEBUG_LOG_ENABLED};
use crate::util::file_store::{
    cancel_uri_requests, mark_parse_pending, mark_parse_done,
    uri_request_token, wait_for_parse_cancellable, FILE_STORE, LSP_WRITER,
};
use crate::util::uri_map::LNG_URI_MAP;
use log::{error, info};
use std::sync::atomic::Ordering;
use tokio_util::sync::CancellationToken;
use url::Url;

/// Map a `MethodCall` variant to its LSP method name string.
fn method_name(call: &MethodCall) -> &'static str {
    match call {
        MethodCall::Initialize(_) => "initialize",
        MethodCall::Shutdown() => "shutdown",
        MethodCall::Exit() => "exit",
        MethodCall::Initialized(_) => "initialized",
        MethodCall::SetTrace(_) => "$/setTrace",
        MethodCall::Cancel(_) => "$/cancelRequest",
        MethodCall::DidClose(_) => "textDocument/didClose",
        MethodCall::DidOpen(_) => "textDocument/didOpen",
        MethodCall::DidChange(_) => "textDocument/didChange",
        MethodCall::DidChangeWatchedFiles(_) => "workspace/didChangeWatchedFiles",
        MethodCall::SemanticFull(_) => "textDocument/semanticTokens/full",
        MethodCall::SemanticRange(_) => "textDocument/semanticTokens/range",
        MethodCall::Diagnostic(_) => "textDocument/diagnostic",
        MethodCall::DocumentSymbol(_) => "textDocument/documentSymbol",
        MethodCall::Folding(_) => "textDocument/foldingRange",
        MethodCall::Completion(_) => "textDocument/completion",
        MethodCall::Hover(_) => "textDocument/hover",
        MethodCall::DocumentHighlight(_) => "textDocument/documentHighlight",
        MethodCall::Definition(_) => "textDocument/definition",
        MethodCall::References(_) => "textDocument/references",
        MethodCall::InlayHint(_) => "textDocument/inlayHint",
        MethodCall::DocumentLink(_) => "textDocument/documentLink",
        MethodCall::Formatting(_) => "textDocument/formatting",
        MethodCall::PrepareRename(_) => "textDocument/prepareRename",
        MethodCall::Rename(_) => "textDocument/rename",
        MethodCall::WillRenameFiles(_) => "workspace/willRenameFiles",
        MethodCall::ImportGraphSubgraph(_) => "importGraph/subgraph",
        MethodCall::CallGraphSubgraph(_) => "callGraph/subgraph",
        MethodCall::TypeGraphSubgraph(_) => "typeGraph/subgraph",
        MethodCall::BuildExecute(_) => "build/execute",
        MethodCall::BuildHooks(_) => "build/hooks",
        MethodCall::RescanExecute(_) => "rescan/execute",
        MethodCall::UjapiDownload(_) => "ujapi/download",
        MethodCall::DocumentColor(_) => "textDocument/documentColor",
        MethodCall::ColorPresentation(_) => "textDocument/colorPresentation",
        MethodCall::CodeAction(_) => "textDocument/codeAction",
        MethodCall::SignatureHelp(_) => "textDocument/signatureHelp",
        MethodCall::CodeLens(_) => "textDocument/codeLens",
        MethodCall::PrepareCallHierarchy(_) => "textDocument/prepareCallHierarchy",
        MethodCall::IncomingCalls(_) => "callHierarchy/incomingCalls",
        MethodCall::OutgoingCalls(_) => "callHierarchy/outgoingCalls",
        MethodCall::PrepareTypeHierarchy(_) => "textDocument/prepareTypeHierarchy",
        MethodCall::Supertypes(_) => "typeHierarchy/supertypes",
        MethodCall::Subtypes(_) => "typeHierarchy/subtypes",
        MethodCall::MpqInfo(_) => "mpq/info",
        MethodCall::MpqList(_) => "mpq/list",
        MethodCall::MpqRead(_) => "mpq/read",
        MethodCall::SlkRender(_) => "slk/render",
        MethodCall::SlkEdit(_) => "slk/edit",
        MethodCall::BlpRender(_) => "blp/render",
        MethodCall::MdxRender(_) => "mdx/render",
        MethodCall::DooRender(_) => "doo/render",
        MethodCall::W3iRender(_) => "w3i/render",
        MethodCall::W3eRender(_) => "w3e/render",
        MethodCall::W3ObjRender(_) => "w3obj/render",
        MethodCall::W3eGamePathSet(_) => "w3e/gamePath/set",
        MethodCall::W3eGamePathStatus(_) => "w3e/gamePath/status",
        MethodCall::W3eTerrainSlk(_) => "w3e/terrainSlk",
        MethodCall::W3eDoodadsSlk(_) => "w3e/doodadsSlk",
        MethodCall::W3eUnitsSlk(_) => "w3e/unitsSlk",
        MethodCall::W3eDestructablesSlk(_) => "w3e/destructablesSlk",
        MethodCall::W3eLookupFile(_) => "w3e/lookupFile",
        MethodCall::DebugLogEnable(_) => "custom/debugLogEnable",
        MethodCall::DebugInit(_) => "custom/debugInit",
    }
}

/// Extract the document URI from a `MethodCall` (if it has one).
///
/// Used to obtain the per-URI request cancellation token **before** the
/// payload is moved into the spawned handler.
fn extract_uri(call: &MethodCall) -> Option<&Url> {
    match call {
        MethodCall::SemanticFull(p) => Some(&p.text_document.uri),
        MethodCall::SemanticRange(p) => Some(&p.text_document.uri),
        MethodCall::DocumentSymbol(p) => Some(&p.text_document.uri),
        MethodCall::Folding(p) => Some(&p.text_document.uri),
        MethodCall::Completion(p) => Some(&p.text_document.uri),
        MethodCall::Hover(p) => Some(&p.text_document.uri),
        MethodCall::DocumentHighlight(p) => Some(&p.text_document.uri),
        MethodCall::Definition(p) => Some(&p.text_document.uri),
        MethodCall::References(p) => Some(&p.text_document.uri),
        MethodCall::InlayHint(p) => Some(&p.text_document.uri),
        MethodCall::DocumentLink(p) => Some(&p.text_document.uri),
        MethodCall::Formatting(p) => Some(&p.text_document.uri),
        MethodCall::PrepareRename(p) => Some(&p.text_document.uri),
        MethodCall::Rename(p) => Some(&p.text_document.uri),
        MethodCall::DocumentColor(p) => Some(&p.text_document.uri),
        MethodCall::ColorPresentation(p) => Some(&p.text_document.uri),
        MethodCall::CodeAction(p) => Some(&p.text_document.uri),
        MethodCall::SignatureHelp(p) => Some(&p.text_document.uri),
        MethodCall::CodeLens(p) => Some(&p.text_document.uri),
        MethodCall::PrepareCallHierarchy(p) => Some(&p.text_document.uri),
        MethodCall::PrepareTypeHierarchy(p) => Some(&p.text_document.uri),
        MethodCall::Diagnostic(p) => Some(&p.text_document.uri),
        _ => None,
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin);
    let writer = Arc::new(Mutex::new(stdout));
    // Set the global writer for background push notifications.
    let _ = LSP_WRITER.set(writer.clone());

    // ── Start binary HTTP server for editor data ─────────────────
    let http_port = crate::http::server::start_server().await.ok();

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
            LspMessage::Call(call) => {
                // ── Debug: log every incoming call ────────────────────
                let m_name = method_name(&call.payload);
                let dbg_uri_str = extract_uri(&call.payload).map(|u| u.to_string());
                send_debug_log(m_name, DebugStatus::Created, &call.id, None, dbg_uri_str).await;

                match call.payload {
                MethodCall::Initialize(_) => {
                    // Store the raw initialize request for debug panel
                    if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&msg) {
                        if let Some(params) = raw.get("params").cloned() {
                            crate::util::debug_log::store_init_request(params);
                        }
                    }

                    let result = InitializeResult {
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
                            color_provider: Some(true),
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
                            signature_help_provider: Some(
                                crate::lsp::signature_help::lsp::SignatureHelpOptions {
                                    trigger_characters: Some(vec![
                                        "(".into(),
                                        ",".into(),
                                    ]),
                                },
                            ),
                            code_lens_provider: Some(
                                crate::lsp::code_lens::lsp::CodeLensOptions {
                                    resolve_provider: Some(false),
                                },
                            ),
                            call_hierarchy_provider: Some(
                                crate::lsp::call_hierarchy::lsp::CallHierarchyOptions {},
                            ),
                            type_hierarchy_provider: Some(
                                crate::lsp::type_hierarchy::lsp::TypeHierarchyOptions {},
                            ),
                            diagnostic_provider: Some(
                                crate::lsp::diagnostic::lsp::DiagnosticOptions {
                                    inter_file_dependencies: Some(true),
                                    workspace_diagnostics: Some(false),
                                },
                            ),
                            ..Default::default()
                        },
                    };

                    // Store the init response for debug panel
                    if let Ok(val) = serde_json::to_value(&result) {
                        crate::util::debug_log::store_init_response(val);
                    }

                    send(
                        &writer,
                        &ResponseMessage {
                            jsonrpc: "2.0".into(),
                            id: call.id,
                            result: Some(result),
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
                            id: call.id,
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

                MethodCall::DidClose(params) => {
                    let uri = params.text_document.uri;

                    // Cancel in-flight request handlers — they target a now-closed file.
                    cancel_uri_requests(&uri);

                    let evicted = crate::util::file_store::evict_closed_file(&uri);

                    // Send empty diagnostics for every evicted URI so the editor
                    // clears stale markers.
                    if !evicted.is_empty() {
                        let writer = Arc::clone(&writer);
                        tokio::spawn(async move {
                            for evicted_uri in &evicted {
                                crate::lsp::send::send(
                                    &writer,
                                    &json!({
                                        "jsonrpc": "2.0",
                                        "method": "textDocument/publishDiagnostics",
                                        "params": {
                                            "uri": evicted_uri.to_string(),
                                            "diagnostics": []
                                        }
                                    }),
                                ).await;
                            }
                        });
                    }
                }

                MethodCall::DebugLogEnable(params) => {
                    DEBUG_LOG_ENABLED.store(params.enabled, Ordering::Relaxed);
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
                    let dbg_method: &'static str = m_name;
                    let dbg_id = call.id.clone();
                    let dbg_uri = extract_uri(&other).map(|u| u.to_string());

                    // Obtain the per-URI cancellation token BEFORE moving
                    // the payload into the spawned task.  When the next
                    // `didChange` for this URI arrives, the token will be
                    // cancelled and the handler bails out immediately.
                    let ct: Option<CancellationToken> =
                        extract_uri(&other).map(|u| uri_request_token(u));

                    tokio::spawn(async move {
                        send_debug_log(dbg_method, DebugStatus::Running, &dbg_id, None, dbg_uri.clone()).await;

                        // ── Early cancellation check ──────────────────────
                        if let Some(ref ct) = ct {
                            if ct.is_cancelled() || call.id.was_cancelled().await {
                                send_cancelled(&writer, call.id).await;
                                return;
                            }
                        }

                        match other {
                            MethodCall::BlpRender(param) => {
                                blp_send(&writer, call.id, &param.uri).await;
                            }

                            MethodCall::MdxRender(param) => {
                                mdx_send(&writer, call.id, &param.uri).await;
                            }

                            MethodCall::DooRender(param) => {
                                doo_send(&writer, call.id, &param.uri, param.is_unit, param.archive_path.as_deref()).await;
                            }

                            MethodCall::W3iRender(param) => {
                                w3i_send(&writer, call.id, &param.uri, param.archive_path.as_deref()).await;
                            }

                            MethodCall::W3eRender(param) => {
                                w3e_send(&writer, call.id, &param.uri, param.archive_path.as_deref()).await;
                            }

                            MethodCall::W3ObjRender(param) => {
                                w3obj_send(&writer, call.id, &param.uri, param.level_data, param.archive_path.as_deref()).await;
                            }

                            MethodCall::W3eGamePathSet(param) => {
                                crate::lng::w3e::game_path::set_game_path(&param.game_path);
                                let status = crate::lng::w3e::game_path::build_status();
                                send(
                                    &writer,
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
                                    &writer,
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
                                    &writer,
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
                                    &writer,
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
                                    &writer,
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
                                    &writer,
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
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(result_val),
                                        error: None,
                                    },
                                ).await;
                            }

                            MethodCall::SlkRender(param) => {
                                slk_send(&writer, call.id, &param.uri).await;
                            }

                            MethodCall::SlkEdit(param) => {
                                let result = crate::lng::slk::edit::apply_cell_edit(&param);
                                send(
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(serde_json::json!(result)),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::Initialized(_) => {
                                // ── Notify extension about binary HTTP server ─────
                                if let Some(port) = http_port {
                                    if let Some(info) = crate::http::server::BINARY_SERVER.get() {
                                        send(
                                            &writer,
                                            &json!({
                                                "jsonrpc": "2.0",
                                                "method": "custom/binaryServerReady",
                                                "params": {
                                                    "port": port,
                                                    "token": info.token
                                                }
                                            }),
                                        )
                                        .await;
                                    }
                                }

                                use crate::util::cache_db;
                                use crate::util::file_cache;
                                use crate::util::import_graph::IMPORT_GRAPH;
                                use crate::util::scope_resolver::SCOPE_RESOLVER;
                                use std::collections::HashSet;

                                // ── 0. Initialize the shared redb database and
                                //       check the cache version stamp. ──────────
                                //       If the extension was updated (version
                                //       mismatch), file_cache and scope tables
                                //       are purged automatically.  The import
                                //       graph is preserved so we still know
                                //       which files belong to which tree.
                                //       Rescanning happens lazily: each tree is
                                //       re-parsed from disk the first time the
                                //       user opens a file from it (via
                                //       ensure_file_symbols → parse from disk).
                                if cache_db::was_purged() {
                                    info!(
                                        "Version changed to {} — data caches purged, \
                                         trees will be rescanned on first open",
                                        cache_db::EXT_VERSION
                                    );
                                }

                                // ── 0a. Force-load the scope resolver from redb ───────
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
                                                colors: Vec::new(),
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
                                                            { "globPattern": "**/*.ai", "kind": 7 },
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
                                let uri = &params.text_document.uri;
                                let ct = ct.as_ref().unwrap();
                                if ct.is_cancelled() || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                if !wait_for_parse_cancellable(uri, Duration::from_secs(5), ct).await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                if ct.is_cancelled() || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                semantic_send(&writer, call.id, uri, None).await
                            }

                            MethodCall::Diagnostic(params) => {
                                let uri = &params.text_document.uri;
                                let ct = ct.as_ref().unwrap();
                                if ct.is_cancelled() || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                if !wait_for_parse_cancellable(uri, Duration::from_secs(5), ct).await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                if ct.is_cancelled() || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let items = FILE_STORE
                                    .get(uri)
                                    .map(|s| s.diagnostics.clone())
                                    .unwrap_or_default();
                                send(
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(json!({
                                            "kind": "full",
                                            "items": items
                                        })),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            MethodCall::SemanticRange(params) => {
                                let uri = &params.text_document.uri;
                                let ct = ct.as_ref().unwrap();
                                if ct.is_cancelled() || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                if !wait_for_parse_cancellable(uri, Duration::from_secs(5), ct).await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                if ct.is_cancelled() || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                semantic_send(&writer, call.id, uri, Some(params.range)).await
                            }


                            MethodCall::DocumentSymbol(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
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
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
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
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                completion_send(&writer, call.id, uri, &params.position).await;
                            }

                            MethodCall::Hover(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                hover_send(&writer, call.id, uri, &params.position).await;
                            }

                            MethodCall::DocumentHighlight(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                highlight_send(&writer, call.id, &params).await;
                            }

                            MethodCall::Definition(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
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
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
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
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                inlay_hint_send(&writer, call.id, &params).await;
                            }

                            MethodCall::DocumentLink(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
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
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                send_formatting(&writer, call.id, &params).await;
                            }

                            MethodCall::PrepareRename(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
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
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
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

                            MethodCall::DocumentColor(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                crate::lsp::color::send::document_color_send(
                                    &writer, call.id, &params,
                                ).await;
                            }

                            MethodCall::ColorPresentation(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                crate::lsp::color::send::color_presentation_send(
                                    &writer, call.id, &params,
                                ).await;
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

                            MethodCall::RescanExecute(params) => {
                                use crate::util::file_cache;
                                use crate::util::import_graph::IMPORT_GRAPH;
                                use crate::util::scope_resolver::SCOPE_RESOLVER;

                                let uri = &params.uri;

                                // ── 1. Resolve the tree for this URI ──────────
                                let tree_uris = IMPORT_GRAPH.tree_for_uri(uri);

                                if tree_uris.is_empty() {
                                    send(
                                        &writer,
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

                            MethodCall::BuildHooks(params) => {
                                let uri = &params.uri;
                                let (before_cmd, after_cmd, cwd) =
                                    crate::lng::jass::build::resolve_hooks(uri);
                                send(
                                    &writer,
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
                                code_action_send(&writer, call.id, &params).await;
                            }

                            MethodCall::SignatureHelp(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                signature_help_send(&writer, call.id, uri, &params.position).await;
                            }

                            MethodCall::CodeLens(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                code_lens_send(&writer, call.id, uri).await;
                            }

                            MethodCall::PrepareCallHierarchy(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                crate::lsp::call_hierarchy::send::send_prepare(
                                    &writer, call.id, uri, &params.position,
                                ).await;
                            }

                            MethodCall::IncomingCalls(params) => {
                                crate::lsp::call_hierarchy::send::send_incoming(
                                    &writer, call.id, &params.item,
                                ).await;
                            }

                            MethodCall::OutgoingCalls(params) => {
                                crate::lsp::call_hierarchy::send::send_outgoing(
                                    &writer, call.id, &params.item,
                                ).await;
                            }

                            MethodCall::PrepareTypeHierarchy(params) => {
                                if ct.as_ref().map_or(false, |t| t.is_cancelled()) || call.id.was_cancelled().await {
                                    send_cancelled(&writer, call.id).await;
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                crate::lsp::type_hierarchy::send::send_prepare(
                                    &writer, call.id, uri, &params.position,
                                ).await;
                            }

                            MethodCall::Supertypes(params) => {
                                crate::lsp::type_hierarchy::send::send_supertypes(
                                    &writer, call.id, &params.item,
                                ).await;
                            }

                            MethodCall::Subtypes(params) => {
                                crate::lsp::type_hierarchy::send::send_subtypes(
                                    &writer, call.id, &params.item,
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
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(result),
                                        error: None,
                                    },
                                ).await;
                            }

                            MethodCall::MpqInfo(params) => {
                                mpq_info_send(&writer, call.id, &params.archive_path).await;
                            }

                            MethodCall::MpqList(params) => {
                                mpq_list_send(&writer, call.id, &params.archive_path).await;
                            }

                            MethodCall::MpqRead(params) => {
                                mpq_read_send(&writer, call.id, &params.archive_path, &params.file_path).await;
                            }

                            MethodCall::DebugInit(_) => {
                                send(
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: call.id,
                                        result: Some(crate::util::debug_log::get_init_data()),
                                        error: None,
                                    },
                                )
                                .await;
                            }

                            _ => {
                                error!("Unexpected method call: {:?}", other);
                            }
                        }
                        send_debug_log(dbg_method, DebugStatus::Completed, &dbg_id, None, dbg_uri).await;
                    });
                }
                }
            },

            LspMessage::RequestMessage(msg) => {
                match msg.method.as_str() {
                    "shutdown" | "exit" => {
                        send(
                            &writer,
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
