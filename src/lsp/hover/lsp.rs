use crate::lsp::position::Position;
use crate::lsp::range::Range;
use crate::lsp::text_document::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#hoverParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#hover
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hover {
    pub contents: MarkupContent,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#markupContent
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkupContent {
    pub kind: MarkupKind,
    pub value: String,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#markupKind
#[derive(Debug, Serialize, Deserialize)]
pub enum MarkupKind {
    #[serde(rename = "plaintext")]
    PlainText,
    #[serde(rename = "markdown")]
    Markdown,
}

