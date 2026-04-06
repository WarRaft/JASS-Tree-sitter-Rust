use crate::lsp::position::Position;
use serde::{Deserialize, Serialize};
use url::Url;


/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#renameFilesParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameFilesParams {
    pub files: Vec<FileRename>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#fileRename
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRename {
    pub old_uri: String,
    pub new_uri: String,
}

// ─── WorkspaceEdit ──────────────────────────────────────────────────────────

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#workspaceEdit
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEdit {
    /// Map of document URI → list of text edits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<std::collections::HashMap<Url, Vec<TextEdit>>>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textEdit
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub range: crate::lsp::range::Range,
    pub new_text: String,
}

// ─── textDocument/rename ─────────────────────────────────────────────────────


/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#renameParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameParams {
    pub text_document: crate::lsp::text_document::TextDocumentIdentifier,
    pub position: Position,
    pub new_name: String,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#prepareRenameParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareRenameParams {
    pub text_document: crate::lsp::text_document::TextDocumentIdentifier,
    pub position: Position,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#prepareRenameResult
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareRenameResult {
    pub range: crate::lsp::range::Range,
    pub placeholder: String,
}
