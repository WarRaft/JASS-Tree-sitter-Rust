use crate::lsp::diagnostic::lsp::Diagnostic;
use crate::lsp::range::Range;
use crate::lsp::rename::lsp::WorkspaceEdit;
use crate::lsp::text_document::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#codeActionParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeActionParams {
    pub text_document: TextDocumentIdentifier,
    pub range: Range,
    pub context: CodeActionContext,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#codeActionContext
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeActionContext {
    pub diagnostics: Vec<Diagnostic>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub only: Option<Vec<String>>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#codeAction
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeAction {
    pub title: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Vec<Diagnostic>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<WorkspaceEdit>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Command>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#command
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub title: String,
    pub command: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<Value>>,
}

/// Parameters for the custom `ujapi/download` request.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UjapiDownloadParams {
    /// URI of the file containing the `//import-ujapi!` directive.
    pub uri: url::Url,
    /// Relative path from the directive.
    pub path: String,
}

/// Code Action kind constants.
pub const CODE_ACTION_KIND_QUICKFIX: &str = "quickfix";
pub const CODE_ACTION_KIND_REFACTOR: &str = "refactor";

