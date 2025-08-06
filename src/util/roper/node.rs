use crate::lsp::position::Position;
use crate::lsp::range::Range;
use lapce_xi_rope::Rope;
use std::borrow::Cow;
use tree_sitter::Node;

pub trait NodeExt {
    fn text<'a>(&self, rope: &'a Rope) -> Cow<'a, str>;
    fn to_range(&self, rope: &Rope) -> Range;
}

impl NodeExt for Node<'_> {
    fn text<'a>(&self, rope: &'a Rope) -> Cow<'a, str> {
        let start = self.start_byte();
        let end = self.end_byte();
        rope.slice_to_cow(start..end)
    }

    fn to_range(&self, rope: &Rope) -> Range {
        let start = Position::from_byte_offset(rope, self.start_byte()).unwrap_or(Position {
            line: 0,
            character: 0,
        });

        let end = Position::from_byte_offset(rope, self.end_byte()).unwrap_or(Position {
            line: 0,
            character: 0,
        });

        Range { start, end }
    }
}
