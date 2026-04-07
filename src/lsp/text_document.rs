use crate::lsp::range::Range;
use serde::{Deserialize, Serialize};
use url::Url;


/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#didCloseTextDocumentParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidCloseTextDocumentParams {
    pub text_document: TextDocumentIdentifier,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocumentIdentifier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentIdentifier {
    pub uri: Url,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocumentContentChangeEvent
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentContentChangeEvent {
    pub range: Range,
    pub text: String,
}

// ─── File watcher types ─────────────────────────────────────────────────────

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#didChangeWatchedFilesParams
#[derive(Debug, Serialize, Deserialize)]
pub struct DidChangeWatchedFilesParams {
    pub changes: Vec<FileEvent>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#fileEvent
#[derive(Debug, Serialize, Deserialize)]
pub struct FileEvent {
    pub uri: Url,
    /// The change type: 1 = Created, 2 = Changed, 3 = Deleted.
    #[serde(rename = "type")]
    pub change_type: u8,
}
