use crate::lsp::position::Position;
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#range
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl From<Node<'_>> for crate::lsp::range::Range {
    fn from(node: Node) -> Self {
        let start = node.start_position();
        let end = node.end_position();

        crate::lsp::range::Range {
            start: Position {
                line: start.row,
                character: start.column,
            },
            end: Position {
                line: end.row,
                character: end.column,
            },
        }
    }
}
