use serde::{Deserialize, Serialize};


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
