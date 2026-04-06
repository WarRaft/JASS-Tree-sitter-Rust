//! Types for document colors and `color/presentation`.

use crate::lsp::range::Range;
use crate::lsp::text_document::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};


/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#colorInformation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorInformation {
    pub range: Range,
    pub color: Color,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#color
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Color {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#colorPresentationParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorPresentationParams {
    pub text_document: TextDocumentIdentifier,
    pub color: Color,
    pub range: Range,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#colorPresentation
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorPresentation {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_edit: Option<TextEdit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_text_edits: Option<Vec<TextEdit>>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textEdit
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

