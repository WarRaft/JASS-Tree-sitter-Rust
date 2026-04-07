use crate::http::position::Position;
use lapce_xi_rope::Rope;
use serde::{Deserialize, Serialize};

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#range
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    /// Build a Range from raw byte offsets into a Rope.
    pub fn from_byte_offsets(rope: &Rope, start_byte: usize, end_byte: usize) -> Self {
        Range {
            start: Position::from_byte_offset(rope, start_byte).unwrap_or_default(),
            end: Position::from_byte_offset(rope, end_byte).unwrap_or_default(),
        }
    }
}

