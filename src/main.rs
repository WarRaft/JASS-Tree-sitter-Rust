pub(crate) mod lsp;
pub(crate) mod util;

pub(crate) mod lng;

use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::{self, BufReader, Stdout};
use tokio::sync::Mutex;

use crate::lsp::diagnostic::lsp::{DiagnosticOptions, DocumentDiagnosticReport};
use crate::lsp::diagnostic::uri_map::URI_MAP;
use crate::lsp::initialize::{InitializeResult, ServerCapabilities};
use crate::lsp::protocol::{LspMessage, MethodCall, ResponseMessage};
use crate::lsp::range::Range;
use crate::lsp::read::read;
use crate::lsp::semantic::lsp::{
    Kind, Mod, SemanticTokens, SemanticTokensFullOptions, SemanticTokensFullOptionsObject,
    SemanticTokensLegend, SemanticTokensOptions, SemanticTokensRangeProviderCapability, ToCamelVec,
};
use crate::lsp::semantic::uri_map::URI_MAP as SEMANTIC_URI_MAP;
use crate::lsp::send::send;
use crate::lsp::text_document::{TextDocumentSyncKind, TextDocumentSyncOptions};
use crate::util::uri_map::LNG_MAP;
use log::error;
use url::Url;

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
                            id: Some(Value::from(call.id)),
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
                            id: Some(json!(null)),
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
                            MethodCall::Initialized(_) => {}
                            MethodCall::SetTrace(_) => {}
                            MethodCall::DidClose(_) => {}
                            MethodCall::DidOpen(params) => {
                                if params.text_document.language_id == "bni" {
                                    lng::bni::open::open(
                                        &params.text_document.uri,
                                        &params.text_document.text,
                                    )
                                    .await;
                                }
                            }

                            MethodCall::DidChange(params) => {
                                let uri = &params.text_document.uri;

                                let lng = {
                                    let map = LNG_MAP.lock().await;
                                    map.get(uri).cloned().flatten()
                                };

                                if let Some(lng) = lng {
                                    if lng == "bni" {
                                        lng::bni::change::change(uri, params.content_changes).await;
                                    }
                                }
                            }

                            MethodCall::SemanticFull(params) => {
                                semantic_token_send(
                                    &writer,
                                    &Value::from(call.id),
                                    &params.text_document.uri,
                                    None,
                                )
                                .await
                            }

                            MethodCall::SemanticRange(params) => {
                                semantic_token_send(
                                    &writer,
                                    &Value::from(call.id),
                                    &params.text_document.uri,
                                    Some(params.range),
                                )
                                .await
                            }

                            MethodCall::Diagnostic(params) => {
                                let uri = &params.text_document.uri;

                                let map = URI_MAP.lock().await;

                                let result = match map.get(uri) {
                                    Some(report) => &report,
                                    None => &DocumentDiagnosticReport::Full {
                                        result_id: None,
                                        items: vec![],
                                        related_documents: None,
                                    },
                                };

                                send(
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: Some(Value::from(call.id)),
                                        result: Some(result),
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

async fn semantic_token_send(
    writer: &Arc<Mutex<Stdout>>,
    call_id: &Value,
    uri: &Url,
    range: Option<Range>,
) {
    let data = {
        let map = SEMANTIC_URI_MAP.lock().await;
        match map.get(uri) {
            Some(semantic) => semantic.data(range),
            None => Vec::new(),
        }
    };

    let _ = send(
        writer,
        &ResponseMessage {
            jsonrpc: "2.0".into(),
            id: Some(call_id.clone()),
            result: Some(SemanticTokens { data }),
            error: None,
        },
    )
    .await;
}
