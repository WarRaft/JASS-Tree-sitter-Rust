use serde::{Deserialize, Serialize};
use tree_sitter::Point;

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#position
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub line: usize,
    pub character: usize,
}

impl Position {
    pub fn point(&self) -> Point {
        Point {
            row: self.line,
            column: self.character,
        }
    }
}