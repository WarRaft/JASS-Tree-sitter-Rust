use crate::lsp::position::Position;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};


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

    /// Byte offset of the AST node this hint is anchored to.
    ///
    /// Used internally to reposition hints instantly after incremental edits
    /// (before the full re-parse completes).  Skipped during JSON serialization.
    #[serde(skip)]
    pub byte_offset: usize,
}

