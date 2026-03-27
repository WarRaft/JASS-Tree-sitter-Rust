use crate::lsp::cancel::{CancelId, CancelParams};
use crate::lsp::code_action::lsp::{CodeActionParams, UjapiDownloadParams};
use crate::lsp::color::lsp::{ColorPresentationParams, DocumentColorParams};
use crate::lsp::completion::lsp::CompletionParams;
use crate::lsp::diagnostic::lsp::DocumentDiagnosticParams;
use crate::lsp::document_link::lsp::DocumentLinkParams;
use crate::lsp::document_symbol::lsp::DocumentSymbolParams;
use crate::lsp::folding::lsp::FoldingRangeParams;
use crate::lsp::formatting::lsp::DocumentFormattingParams;
use crate::lsp::highlight::lsp::{DefinitionParams, DocumentHighlightParams, ReferenceParams};
use crate::lsp::hover::lsp::HoverParams;
use crate::lsp::inlay_hint::lsp::InlayHintParams;
use crate::lsp::initialize::InitializeParams;
use crate::lsp::initialized::InitializedParams;
use crate::lsp::rename::lsp::{PrepareRenameParams, RenameFilesParams, RenameParams};
use crate::lsp::semantic::lsp::{SemanticTokensParams, SemanticTokensRangeParams};
use crate::lsp::set_trace::SetTraceParams;
use crate::lsp::text_document::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, TextDocumentIdentifier,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#requestMessage
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LspMessage {
    Call(LspCall),
    RequestMessage(RequestMessage),
    /// Response from the client to a server-initiated request
    /// (e.g. `workspace/semanticTokens/refresh`).  Silently consumed.
    ClientResponse(ClientResponse),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientResponse {
    pub id: Value,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<Value>,
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
    /// BLP
    #[serde(rename = "blp/render")]
    BlpRender(TextDocumentIdentifier),

    /// DOO
    #[serde(rename = "doo/render")]
    DooRender(TextDocumentIdentifier),

    /// W3I
    #[serde(rename = "w3i/render")]
    W3iRender(W3iRenderParams),

    /// LSP
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

    #[serde(rename = "workspace/didChangeWatchedFiles")]
    DidChangeWatchedFiles(DidChangeWatchedFilesParams),

    #[serde(rename = "textDocument/semanticTokens/full")]
    SemanticFull(SemanticTokensParams),

    #[serde(rename = "textDocument/semanticTokens/range")]
    SemanticRange(SemanticTokensRangeParams),

    #[serde(rename = "textDocument/diagnostic")]
    Diagnostic(DocumentDiagnosticParams),

    #[serde(rename = "textDocument/documentSymbol")]
    DocumentSymbol(DocumentSymbolParams),

    #[serde(rename = "textDocument/foldingRange")]
    Folding(FoldingRangeParams),

    #[serde(rename = "textDocument/completion")]
    Completion(CompletionParams),

    #[serde(rename = "textDocument/hover")]
    Hover(HoverParams),

    #[serde(rename = "textDocument/documentHighlight")]
    DocumentHighlight(DocumentHighlightParams),

    #[serde(rename = "textDocument/definition")]
    Definition(DefinitionParams),

    #[serde(rename = "textDocument/references")]
    References(ReferenceParams),

    #[serde(rename = "textDocument/inlayHint")]
    InlayHint(InlayHintParams),

    #[serde(rename = "textDocument/documentLink")]
    DocumentLink(DocumentLinkParams),

    #[serde(rename = "textDocument/formatting")]
    Formatting(DocumentFormattingParams),

    #[serde(rename = "textDocument/prepareRename")]
    PrepareRename(PrepareRenameParams),

    #[serde(rename = "textDocument/rename")]
    Rename(RenameParams),

    #[serde(rename = "workspace/willRenameFiles")]
    WillRenameFiles(RenameFilesParams),

    #[serde(rename = "importGraph/subgraph")]
    ImportGraphSubgraph(TextDocumentIdentifier),

    #[serde(rename = "callGraph/subgraph")]
    CallGraphSubgraph(TextDocumentIdentifier),

    #[serde(rename = "typeGraph/subgraph")]
    TypeGraphSubgraph(TextDocumentIdentifier),

    #[serde(rename = "build/execute")]
    BuildExecute(TextDocumentIdentifier),

    #[serde(rename = "rescan/execute")]
    RescanExecute(TextDocumentIdentifier),

    #[serde(rename = "ujapi/download")]
    UjapiDownload(UjapiDownloadParams),

    #[serde(rename = "textDocument/documentColor")]
    DocumentColor(DocumentColorParams),

    #[serde(rename = "textDocument/colorPresentation")]
    ColorPresentation(ColorPresentationParams),

    #[serde(rename = "textDocument/codeAction")]
    CodeAction(CodeActionParams),

    /// MPQ archive browsing
    #[serde(rename = "mpq/info")]
    MpqInfo(MpqArchiveParams),

    #[serde(rename = "mpq/list")]
    MpqList(MpqArchiveParams),

    #[serde(rename = "mpq/read")]
    MpqRead(MpqReadParams),

    /// SLK table editor
    #[serde(rename = "slk/render")]
    SlkRender(TextDocumentIdentifier),

    #[serde(rename = "slk/edit")]
    SlkEdit(SlkEditParams),

    /// Debug log toggle
    #[serde(rename = "custom/debugLogEnable")]
    DebugLogEnable(DebugLogEnableParams),

    /// Debug init data request
    #[serde(rename = "custom/debugInit")]
    DebugInit(serde_json::Value),
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#requestMessage
#[derive(Debug, Serialize, Deserialize)]
pub struct RequestMessage {
    pub id: Value,
    pub method: String,
    pub params: Option<Value>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#responseMessage
#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseMessage<T = Value> {
    pub jsonrpc: String,
    pub id: Option<CancelId>,
    pub result: Option<T>,
    pub error: Option<Value>,
}

/// Params for `mpq/list` — list files in an archive.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MpqArchiveParams {
    /// Absolute filesystem path to the MPQ archive.
    pub archive_path: String,
}

/// Params for `mpq/read` — read a single file from an archive.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MpqReadParams {
    /// Absolute filesystem path to the MPQ archive.
    pub archive_path: String,
    /// Internal path inside the archive (e.g. "war3map.j").
    pub file_path: String,
}

/// Params for `w3i/render`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct W3iRenderParams {
    pub uri: Url,
    /// When opened from an MPQ archive, the absolute path to the archive
    /// so the server can read `war3map.wts` and resolve TRIGSTR references.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_path: Option<String>,
}

/// Params for `slk/edit` — edit a single cell in the SLK table.
#[derive(Debug, Serialize, Deserialize)]
pub struct SlkEditParams {
    pub uri: Url,
    /// Byte offset of the old value in the document.
    pub start: usize,
    /// Byte length of the old value.
    pub len: usize,
    /// New cell value (raw text to insert).
    pub value: String,
}

/// Params for `custom/debugLogEnable` — toggle debug log streaming.
#[derive(Debug, Serialize, Deserialize)]
pub struct DebugLogEnableParams {
    pub enabled: bool,
}

