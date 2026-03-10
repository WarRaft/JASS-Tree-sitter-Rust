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
use crate::lsp::diagnostic::lsp::{DiagnosticOptions, DocumentDiagnosticReport};
use crate::lsp::diagnostic::uri_map::URI_MAP as DIAGNOSTIC_URI_MAP;
use crate::lsp::document_link::lsp::DocumentLinkOptions;
use crate::lsp::document_link::uri_map::URI_MAP as LINK_URI_MAP;
use crate::lsp::document_symbol::lsp::DocumentSymbolOptions;
use crate::lsp::document_symbol::uri_map::URI_MAP as SYMBOL_URI_MAP;
use crate::lsp::folding::lsp::FoldingRangeOptions;
use crate::lsp::folding::uri_map::URI_MAP as FOLDING_URI_MAP;
use crate::lsp::initialize::{InitializeResult, ServerCapabilities};
use crate::lsp::protocol::{LspMessage, MethodCall, ResponseMessage};
use crate::lsp::read::read;
use crate::lsp::rename::handle::compute_rename_edits;
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
use crate::lsp::text_document::{TextDocumentSyncKind, TextDocumentSyncOptions};
use crate::util::uri_lock::uri_wait;
use crate::util::uri_map::LNG_URI_MAP;
use log::error;

#[tokio::main]
async fn main() {
    env_logger::init();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin);
    let writer = Arc::new(Mutex::new(stdout));

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
                                        inter_file_dependencies: false,
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
                                    document_link_provider: Some(DocumentLinkOptions {
                                        resolve_provider: Some(false),
                                    }),
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

                other => {
                    tokio::spawn(async move {
                        match other {
                            MethodCall::BlpRender(param) => {
                                blp_send(&writer, call.id, &param.uri).await;
                            }

                            MethodCall::Initialized(_) => {}
                            MethodCall::SetTrace(_) => {}
                            MethodCall::DidClose(_) => {}
                            MethodCall::Cancel(params) => {
                                params.id.mark_cancelled().await;
                            }

                            MethodCall::DidOpen(params) => {
                                if params.text_document.language_id == "bni" {
                                    if let Err(err) = lng::bni::open::open(
                                        &params.text_document.uri,
                                        &params.text_document.text,
                                    )
                                    .await
                                    {
                                        error!("Failed to apply change: {}", err);
                                    }
                                } else if params.text_document.language_id == "jass" {
                                    if let Err(err) = lng::jass::open::open(
                                        &params.text_document.uri,
                                        &params.text_document.text,
                                    )
                                    .await
                                    {
                                        error!("Failed to apply change: {}", err);
                                    }
                                } else if params.text_document.language_id == "angelscript" {
                                    if let Err(err) = lng::ass::open::open(
                                        &params.text_document.uri,
                                        &params.text_document.text,
                                    )
                                    .await
                                    {
                                        error!("Failed to apply change: {}", err);
                                    }
                                }
                            }

                            MethodCall::DidChange(params) => {
                                let uri = &params.text_document.uri;

                                if let Some(lng) = LNG_URI_MAP.get(uri) {
                                    if lng.value() == "bni" {
                                        if let Err(err) =
                                            lng::bni::change::change(uri, params.content_changes)
                                                .await
                                        {
                                            error!("Failed to apply change: {}", err);
                                        }
                                    } else if lng.value() == "jass" {
                                        if let Err(err) =
                                            lng::jass::change::change(uri, params.content_changes)
                                                .await
                                        {
                                            error!("Failed to apply change: {}", err);
                                        }
                                        // Notify client to re-request semantic tokens
                                        send(
                                            &writer,
                                            &json!({
                                                "jsonrpc": "2.0",
                                                "method": "workspace/semanticTokens/refresh"
                                            }),
                                        )
                                        .await;
                                    } else if lng.value() == "angelscript" {
                                        if let Err(err) =
                                            lng::ass::change::change(uri, params.content_changes)
                                                .await
                                        {
                                            error!("Failed to apply change: {}", err);
                                        }
                                        send(
                                            &writer,
                                            &json!({
                                                "jsonrpc": "2.0",
                                                "method": "workspace/semanticTokens/refresh"
                                            }),
                                        )
                                        .await;
                                    }
                                }
                            }

                            MethodCall::SemanticFull(params) => {
                                if call.id.was_cancelled().await {
                                    return;
                                }
                                let uri = &params.text_document.uri;

                                uri_wait(uri).await;

                                semantic_send(&writer, call.id, uri, None).await
                            }

                            MethodCall::SemanticRange(params) => {
                                if call.id.was_cancelled().await {
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                uri_wait(uri).await;

                                semantic_send(&writer, call.id, uri, Some(params.range)).await
                            }

                            MethodCall::Diagnostic(params) => {
                                if call.id.was_cancelled().await {
                                    return;
                                }

                                let uri = &params.text_document.uri;
                                uri_wait(uri).await;

                                let default = DocumentDiagnosticReport::Full {
                                    result_id: None,
                                    items: vec![],
                                    related_documents: None,
                                };
                                let result_ref;
                                let result: &DocumentDiagnosticReport;

                                if let Some(entry) = DIAGNOSTIC_URI_MAP.get(uri) {
                                    result_ref = entry;
                                    result = result_ref.value();
                                } else {
                                    result = &default;
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
                                .await;
                            }

                            MethodCall::DocumentSymbol(params) => {
                                if call.id.was_cancelled().await {
                                    return;
                                }

                                let uri = &params.text_document.uri;
                                uri_wait(uri).await;

                                let result_ref;
                                let result: Option<&_> =
                                    if let Some(entry) = SYMBOL_URI_MAP.get(uri) {
                                        result_ref = entry; // держим Ref живым
                                        Some(result_ref.value()) // возвращаем ссылку, живущую столько же
                                    } else {
                                        None
                                    };

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
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                uri_wait(uri).await;

                                let result_ref;
                                let result = match FOLDING_URI_MAP.get(uri) {
                                    Some(r) => {
                                        result_ref = r;
                                        result_ref.value()
                                    }
                                    None => &[].into(),
                                };

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
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                completion_send(&writer, call.id, uri, &params.position).await;
                            }

                            MethodCall::DocumentLink(params) => {
                                if call.id.was_cancelled().await {
                                    return;
                                }
                                let uri = &params.text_document.uri;
                                uri_wait(uri).await;

                                let result_ref;
                                let result: &Vec<crate::lsp::document_link::lsp::DocumentLink> =
                                    match LINK_URI_MAP.get(uri) {
                                        Some(r) => {
                                            result_ref = r;
                                            result_ref.value()
                                        }
                                        None => &vec![],
                                    };

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

                            _ => {
                                error!("Unexpected method call: {:?}", other);
                            }
                        }
                    });
                }
            },

            LspMessage::RequestMessage(msg) => {
                error!("Unexpected request: {:?}", msg);
            }
        }
    }

    std::process::exit(0);
}
