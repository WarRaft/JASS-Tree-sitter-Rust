pub mod initialize;
pub mod initialized;
pub mod set_trace;
mod text_document;

use crate::lsp::initialize::InitializeParams;
use crate::lsp::initialized::InitializedParams;
use crate::lsp::set_trace::SetTraceParams;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LspMessage {
    Call(LspCall),
    Response(ResponseMessage<Value>),
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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseMessage<T = Value> {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Option<T>,
    pub error: Option<Value>,
}
