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
use crate::util::uri_map::URI_MAP;
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
                error!("Failed to parse message: {}", err);
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
                                let mut map = URI_MAP.lock().await;
                                let lng = map.entry(&uri).lng.as_ref().cloned();
                                drop(map);
                                if let Some(lng) = lng {
                                    if lng == "bni" {
                                        lng::bni::change::change(&uri, params.content_changes)
                                            .await;
                                    }
                                }
                            }

                            MethodCall::SemanticFull(params) => {
                                let mut map = URI_MAP.lock().await;
                                let semantic = map.entry(&params.text_document.uri).semantic;
                                send(
                                    &writer,
                                    &ResponseMessage {
                                        jsonrpc: "2.0".into(),
                                        id: Some(Value::from(call.id)),
                                        result: Some(SemanticTokens {
                                            data: semantic.data(),
                                        }),
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
