use serde::{Deserialize, Serialize};
use url::Url;

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#didOpenTextDocumentParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidOpenTextDocumentParams {
    pub text_document: TextDocumentItem,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocumentItem
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentItem {
    pub uri: Url,
    pub language_id: String,
    pub version: i32,
    pub text: String,
}
