use crate::lsp::range::Range;
use crate::lsp::text_document::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#documentLinkOptions
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLinkOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve_provider: Option<bool>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#documentLinkParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLinkParams {
    pub text_document: TextDocumentIdentifier,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#documentLink
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLink {
    /// The range this link applies to.
    pub range: Range,

    /// The uri this link points to. If missing a resolve request is sent later.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,

    /// An optional tooltip shown when hovering over the link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
}

