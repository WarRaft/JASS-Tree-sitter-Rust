use serde::{Deserialize, Serialize};
use tree_sitter::Point;

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#position
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub line: usize,
    pub character: usize,
}

impl From<Point> for Position {
    fn from(p: Point) -> Self {
        Position {
            line: p.row,
            character: p.column,
        }
    }
}

impl From<Position> for Point {
    fn from(p: Position) -> Self {
        Point {
            row: p.line,
            column: p.character,
        }
    }
}

impl From<&Position> for Point {
    fn from(p: &Position) -> Self {
        Point {
            row: p.line,
            column: p.character,
        }
    }
}