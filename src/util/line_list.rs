use crate::lsp::position::Position;
use ropey::Rope;
use std::str;
use tree_sitter::Point;

#[derive(Clone, Debug)]
pub struct LineList {
    pub rope: Rope,
}

impl LineList {
    pub fn new() -> Self {
        Self { rope: Rope::new() }
    }

    pub fn set_text(&mut self, text: &str) {
        self.rope = Rope::from_str(text);
    }

    pub fn to_text(&self) -> String {
        self.rope.to_string()
    }

    pub fn position_to_char(&self, pos: &Position) -> usize {
        let line = pos.line.min(self.rope.len_lines().saturating_sub(1));
        let line_start = self.rope.line_to_char(line);

        let col = pos.character;
        let line_len = self.rope.line(line).chars().count();
        let safe_col = col.min(line_len);

        line_start + safe_col
    }

    pub fn position_to_offset(&self, pos: &Position) -> Option<usize> {
        let char_idx = self.position_to_char(pos);
        Some(self.rope.char_to_byte(char_idx))
    }

    pub fn point_from_offset(&self, offset: usize) -> Point {
        let byte = offset.min(self.rope.len_bytes());
        let char_idx = self.rope.byte_to_char(byte);
        let line = self.rope.char_to_line(char_idx);
        let col = char_idx - self.rope.line_to_char(line);
        Point {
            row: line,
            column: col,
        }
    }

    pub fn apply_change(&mut self, start: &Position, end: &Position, new_text: &str) {
        let start_char = self.position_to_char(start);
        let end_char = self.position_to_char(end);

        if start_char > end_char || end_char > self.rope.len_chars() {
            log::warn!(
                "Skipping invalid change: start_char={}, end_char={}, len_chars={}",
                start_char,
                end_char,
                self.rope.len_chars()
            );
            return;
        }

        self.rope.remove(start_char..end_char);
        self.rope.insert(start_char, new_text);
    }
}
