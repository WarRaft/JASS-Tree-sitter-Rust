use crate::lsp::position::Position;
use crate::lsp::text_document::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};


/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#completionParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#completionList
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionList {
    pub is_incomplete: bool,
    pub items: Vec<CompletionItem>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#completionItem
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<CompletionItemKind>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_text_format: Option<InsertTextFormat>,

    /// When true, `label` is used for display and `insert_text` for insertion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_text: Option<String>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#insertTextFormat
#[derive(Debug, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum InsertTextFormat {
    PlainText = 1,
    Snippet = 2,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#completionItemKind
#[derive(Debug, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum CompletionItemKind {
    Text = 1,
    Method = 2,
    Function = 3,
    Constructor = 4,
    Field = 5,
    Variable = 6,
    Class = 7,
    Interface = 8,
    Module = 9,
    Property = 10,
    Unit = 11,
    Value = 12,
    Enum = 13,
    Keyword = 14,
    Snippet = 15,
    Color = 16,
    File = 17,
    Reference = 18,
    Folder = 19,
    EnumMember = 20,
    Constant = 21,
    Struct = 22,
    Event = 23,
    Operator = 24,
    TypeParameter = 25,
}

