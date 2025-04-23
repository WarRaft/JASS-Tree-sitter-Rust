mod lsp;

use crate::lsp::initialize::{
    InitializeResult, SemanticTokensLegend, SemanticTokensOptions, TextDocumentSyncKind,
    TextDocumentSyncOptions, ToCamelVec, TokenModifier, TokenType,
};
use crate::lsp::set_trace::SetTraceParams;
use crate::lsp::{LspMessage, MethodCall, ResponseMessage};
use initialize::ServerCapabilities;
use log::{error, info};
use lsp::initialize;
use serde::Serialize;
use serde_json::json;
use std::io::{self, BufRead, BufReader, Write};

fn main() {
    env_logger::init();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    info!("LSP server started");

    while let Some(msg) = lsp_read(&mut reader) {
        info!("<<< {}", msg);

        match serde_json::from_str::<LspMessage>(&msg) {
            Ok(LspMessage::Call(call)) => match call.id {
                Some(id) => match call.payload {
                    MethodCall::Initialize(params) => {
                        info!("Initialize Params: {:?}", params);

                        lsp_send(
                            &mut writer,
                            &ResponseMessage {
                                jsonrpc: "2.0".into(),
                                id,
                                result: Some(InitializeResult {
                                    capabilities: ServerCapabilities {
                                        text_document_sync: Some(TextDocumentSyncOptions {
                                            open_close: Some(true),
                                            change: Some(TextDocumentSyncKind::Incremental),
                                        }),
                                        semantic_tokens_provider: Some(SemanticTokensOptions {
                                            legend: SemanticTokensLegend {
                                                token_types: <TokenType as ToCamelVec>::get_vec(),
                                                token_modifiers:
                                                    <TokenModifier as ToCamelVec>::get_vec(),
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

                    MethodCall::Shutdown() => {
                        lsp_send(
                            &mut writer,
                            &ResponseMessage {
                                jsonrpc: "2.0".into(),
                                id,
                                result: Some(json!(null)),
                                error: None,
                            },
                        );
                        break;
                    }
                    MethodCall::Exit() => {
                        lsp_send(
                            &mut writer,
                            &ResponseMessage {
                                jsonrpc: "2.0".into(),
                                id,
                                result: Some(json!(null)),
                                error: None,
                            },
                        );
                        break;
                    }

                    _ => {
                        error!("Unknown request method");
                    }
                },

                None => match call.payload {
                    MethodCall::Initialized(_) => {
                        info!("Received 'initialized' notification");
                    }
                    MethodCall::SetTrace(SetTraceParams { value }) => {
                        info!("Set trace level: {:?}", value);
                    }
                    _ => {
                        info!("Unknown notification method");
                    }
                },
            },

            Ok(LspMessage::Response(_)) => {}

            Err(err) => {
                error!("Failed to parse message: {}", err);
            }
        }
    }

    info!("LSP server shutting down (stdin closed)");
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
