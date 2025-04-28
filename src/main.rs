pub mod lng;
pub mod lsp;

use crate::lsp::initialize::InitializeResult;
use crate::lsp::semantic::{
    SemanticTokens, SemanticTokensLegend, SemanticTokensOptions, ToCamelVec, TokenModifier,
    TokenType,
};
use crate::lsp::semantic_hub::SEMANTIC_STORE;
use crate::lsp::text_document::{TextDocumentSyncKind, TextDocumentSyncOptions};
use crate::lsp::{LspMessage, MethodCall, ResponseMessage};
use initialize::ServerCapabilities;
use log::{error, info};
use lsp::initialize;
use serde::Serialize;
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Write};

fn main() {
    env_logger::init();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    while let Some(msg) = lsp_read(&mut reader) {
        match serde_json::from_str::<LspMessage>(&msg) {
            Ok(LspMessage::Call(call)) => match call.payload {
                MethodCall::Initialize(_) => {
                    lsp_send(
                        &mut writer,
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
                    );
                }
                MethodCall::Shutdown() | MethodCall::Exit() => {
                    lsp_send(
                        &mut writer,
                        &ResponseMessage {
                            jsonrpc: "2.0".into(),
                            id: Some(json!(null)),
                            result: Some(json!(null)),
                            error: None,
                        },
                    );
                    break;
                }

                MethodCall::Initialized(_) => {}
                MethodCall::SetTrace(_) => {}

                MethodCall::DidOpen(params) => {
                    if params.text_document.language_id == "bni" {
                        lng::bni::parse(&params.text_document.uri, &params.text_document.text);
                    }
                }

                MethodCall::DidChange(params) => {
                    info!(
                        "Received 'didChangeTextDocument' notification: {:?}",
                        params
                    );
                }

                MethodCall::SemanticFull(params) => {
                    let mut store = SEMANTIC_STORE.lock().unwrap();
                    let hub = store.hub(&params.text_document.uri);
                    lsp_send(
                        &mut writer,
                        &ResponseMessage {
                            jsonrpc: "2.0".into(),
                            id: Some(Value::from(call.id)),
                            result: Some(SemanticTokens { data: hub.data() }),
                            error: None,
                        },
                    );
                }
            },

            Ok(LspMessage::RequestMessage(msg)) => {
                error!("Unexpected request: {:?}", msg);
            }

            Err(err) => {
                error!("Failed to parse message: {}", err);
            }
        }
    }

    std::process::exit(0);
}

fn lsp_read<R: BufRead>(reader: &mut R) -> Option<String> {
    let mut content_length = 0;
    let mut line = String::new();

    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        if line == "\r\n" {
            break;
        } else if let Some(cl) = line.strip_prefix("Content-Length:") {
            content_length = cl.trim().parse::<usize>().ok()?;
        }
    }

    let mut body = vec![0; content_length];
    reader.read_exact(&mut body).ok()?;
    Some(String::from_utf8(body).ok()?)
}

fn lsp_send<T: Serialize, W: Write>(writer: &mut W, message: &T) {
    let msg = serde_json::to_string(message).unwrap();
    write!(writer, "Content-Length: {}\r\n\r\n", msg.len()).unwrap();
    writer.write_all(msg.as_bytes()).unwrap();
    writer.flush().unwrap();
}
