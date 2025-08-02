use crate::lsp::text_document::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#foldingRangeOptions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldingRangeOptions {}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#foldingRangeParams
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldingRangeParams {
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldingRange {
    pub start_line: usize,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_character: Option<usize>,

    pub end_line: usize,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_character: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<FoldingRangeKind>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub collapsed_text: Option<String>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/specification-current/#foldingRangeKind
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FoldingRangeKind {
    Comment,
    Imports,
    Region,
}
