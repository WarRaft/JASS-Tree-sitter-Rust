use crate::lsp::position::Position;
use serde::{Deserialize, Serialize};

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#range
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub start: Position,
    pub end: Position,
}
