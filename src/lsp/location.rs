use crate::lsp::range::Range;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct LocationLink {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_selection_range: Option<Range>,

    pub target_uri: String,

    pub target_range: Range,

    pub target_selection_range: Range,
}
