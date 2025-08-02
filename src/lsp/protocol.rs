use crate::lsp::cancel::{CancelId, CancelParams};
use crate::lsp::diagnostic::lsp::DocumentDiagnosticParams;
use crate::lsp::document_symbol::lsp::DocumentSymbolParams;
use crate::lsp::initialize::InitializeParams;
use crate::lsp::initialized::InitializedParams;
use crate::lsp::semantic::lsp::{SemanticTokensParams, SemanticTokensRangeParams};
use crate::lsp::set_trace::SetTraceParams;
use crate::lsp::text_document::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#requestMessage
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LspMessage {
    Call(LspCall),
    RequestMessage(RequestMessage),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LspCall {
    pub jsonrpc: String,
    pub id: Option<CancelId>,
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

    #[serde(rename = "$/cancelRequest")]
    Cancel(CancelParams),

    #[serde(rename = "textDocument/didClose")]
    DidClose(DidCloseTextDocumentParams),

    #[serde(rename = "textDocument/didOpen")]
    DidOpen(DidOpenTextDocumentParams),

    #[serde(rename = "textDocument/didChange")]
    DidChange(DidChangeTextDocumentParams),

    #[serde(rename = "textDocument/semanticTokens/full")]
    SemanticFull(SemanticTokensParams),

    #[serde(rename = "textDocument/semanticTokens/range")]
    SemanticRange(SemanticTokensRangeParams),

    #[serde(rename = "textDocument/diagnostic")]
    Diagnostic(DocumentDiagnosticParams),

    #[serde(rename = "textDocument/documentSymbol")]
    DocumentSymbol(DocumentSymbolParams),
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
    pub id: Option<CancelId>,
    pub result: Option<T>,
    pub error: Option<Value>,
}
