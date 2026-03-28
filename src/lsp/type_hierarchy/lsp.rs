use crate::lsp::document_symbol::lsp::SymbolKind;
use crate::lsp::position::Position;
use crate::lsp::range::Range;
use crate::lsp::text_document::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#typeHierarchyPrepareParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeHierarchyPrepareParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#typeHierarchyItem
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeHierarchyItem {
    pub name: String,
    pub kind: SymbolKind,
    pub uri: String,
    pub range: Range,
    pub selection_range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#typeHierarchySupertypesParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeHierarchySupertypesParams {
    pub item: TypeHierarchyItem,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#typeHierarchySubtypesParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeHierarchySubtypesParams {
    pub item: TypeHierarchyItem,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#typeHierarchyOptions
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeHierarchyOptions {}

