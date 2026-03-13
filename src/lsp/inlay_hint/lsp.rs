use crate::lsp::position::Position;
use crate::lsp::range::Range;
use crate::lsp::text_document::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#inlayHintParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlayHintParams {
    pub text_document: TextDocumentIdentifier,
    pub range: Range,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#inlayHintKind
#[derive(Debug, Serialize_repr, Deserialize_repr, Clone, Copy)]
#[repr(u8)]
pub enum InlayHintKind {
    Type = 1,
    Parameter = 2,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#inlayHint
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InlayHint {
    pub position: Position,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<InlayHintKind>,
    /// Render padding before the hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding_left: Option<bool>,
    /// Render padding after the hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding_right: Option<bool>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#inlayHintOptions
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlayHintOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve_provider: Option<bool>,
}

