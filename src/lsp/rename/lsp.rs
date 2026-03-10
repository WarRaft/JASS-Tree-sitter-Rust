use serde::{Deserialize, Serialize};
use url::Url;

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#fileOperationRegistrationOptions
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOperationRegistrationOptions {
    pub filters: Vec<FileOperationFilter>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#fileOperationFilter
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOperationFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    pub pattern: FileOperationPattern,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#fileOperationPattern
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOperationPattern {
    pub glob: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<FileOperationPatternKind>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#fileOperationPatternKind
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileOperationPatternKind {
    File,
    Folder,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#fileOperationOptions
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOperationOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub will_rename: Option<FileOperationRegistrationOptions>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#serverCapabilities  workspace
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_operations: Option<FileOperationOptions>,
}

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

