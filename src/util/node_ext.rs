use crate::lsp::position::Position;
use crate::lsp::range::Range;
use tree_sitter::Node;

pub trait NodeExt {
    fn range_lsp(&self) -> Range;
}

impl NodeExt for Node<'_> {
    fn range_lsp(&self) -> Range {
        let s = self.start_position();
        let e = self.end_position();
        Range {
            start: Position {
                line: s.row,
                character: s.column,
            },
            end: Position {
                line: e.row,
                character: e.column,
            },
        }
    }
}
