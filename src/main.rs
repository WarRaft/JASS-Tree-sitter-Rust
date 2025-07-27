pub(crate) mod lsp;
pub(crate) mod util;

pub(crate) mod lng;

use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::{self, BufReader};
use tokio::sync::Mutex;

use crate::lsp::initialize::{InitializeResult, ServerCapabilities};
use crate::lsp::protocol::{LspMessage, MethodCall, ResponseMessage};
use crate::lsp::read::read;
use crate::lsp::semantic::{
    SemanticTokens, SemanticTokensLegend, SemanticTokensOptions, ToCamelVec, TokenModifier,
    TokenType,
};
use crate::lsp::send::send;
use crate::lsp::text_document::{TextDocumentSyncKind, TextDocumentSyncOptions};
use crate::util::uri_map::{LNG_MAP, SEMANTIC_MAP};
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

        let writer = writer.clone();

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
                                            token_types: <TokenType as ToCamelVec>::get_vec(),
                                            token_modifiers: <TokenModifier as ToCamelVec>::get_vec(
                                            ),
                                        },
                                        full: true,
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
                                let uri = &params.text_document.uri;

                                let data = {
                                    let map = SEMANTIC_MAP.lock().await;
                                    match map.get(uri) {
                                        Some(semantic) => semantic.data(),
                                        None => Vec::new(), // если семантика не проинициализирована
                                    }
                                };

                                send(
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: Some(Value::from(call.id)),
                                        result: Some(SemanticTokens { data }),
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
