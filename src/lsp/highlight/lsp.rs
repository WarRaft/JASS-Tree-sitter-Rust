use crate::lsp::position::Position;
use crate::lsp::range::Range;
use crate::lsp::text_document::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#documentHighlightParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentHighlightParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#documentHighlightKind
#[derive(Debug, Serialize_repr, Deserialize_repr, Clone, Copy)]
#[repr(u8)]
pub enum DocumentHighlightKind {
    /// A textual occurrence.
    Text = 1,
    /// Read-access of a symbol (e.g. reading a variable).
    Read = 2,
    /// Write-access of a symbol (e.g. writing to a variable).
    Write = 3,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#documentHighlight
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DocumentHighlight {
    pub range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<DocumentHighlightKind>,
}

// ─── Definition ──────────────────────────────────────────────────────────────

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#definitionParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

// ─── References ──────────────────────────────────────────────────────────────

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#referenceParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
    #[serde(default)]
    pub context: ReferenceContext,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#referenceContext
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceContext {
    pub include_declaration: bool,
}

