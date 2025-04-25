pub mod initialize;
pub mod initialized;
pub mod set_trace;
mod text_document;

use crate::lsp::initialize::InitializeParams;
use crate::lsp::initialized::InitializedParams;
use crate::lsp::set_trace::SetTraceParams;
use crate::lsp::text_document::DidOpenTextDocumentParams;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LspMessage {
    Call(LspCall),
    RequestMessage(RequestMessage),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LspCall {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(flatten)]
    pub payload: MethodCall,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum MethodCall {
    #[serde(rename = "initialize")]
    Initialize(InitializeParams),

    #[serde(rename = "shutdown")]
    Shutdown(),

    #[serde(rename = "exit")]
    Exit(),

    #[serde(rename = "initialized")]
    Initialized(InitializedParams),

    #[serde(rename = "$/setTrace")]
    SetTrace(SetTraceParams),

    #[serde(rename = "textDocument/didOpen")]
    DidOpen(DidOpenTextDocumentParams),
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#requestMessage
#[derive(Debug, Serialize, Deserialize)]
pub struct RequestMessage {
    id: Value,
    method: String,
    params: Option<Value>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#responseMessage
#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseMessage<T = Value> {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub result: Option<T>,
    pub error: Option<Value>,
}
