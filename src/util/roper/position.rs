use crate::http::position::Position;
use lapce_xi_rope::{LinesMetric, Rope};
use tree_sitter::Point;

impl Position {
    /// Переводит LSP-позицию (line, character в UTF-16) в байтовый offset Rope
    pub fn to_byte_offset(&self, rope: &Rope) -> Option<usize> {
        let total_lines = rope.measure::<LinesMetric>() + 1;
        if self.line >= total_lines {
            return None;
        }
        let line_start = rope.offset_of_line(self.line);
        let line_end = rope.offset_of_line(self.line + 1).min(rope.len());
        let line_text = rope.slice(line_start..line_end).to_string();

        let mut utf16_i = 0;
        let mut byte_in_line = 0;

        for ch in line_text.chars() {
            let ch_utf16_len = ch.len_utf16();
            if utf16_i + ch_utf16_len > self.character as usize {
                break;
            }
            utf16_i += ch_utf16_len;
            byte_in_line += ch.len_utf8();
        }

        // если utf16_i не совпадает с self.character, значит попали внутрь суррогата — невалидно
        if utf16_i != self.character as usize {
            return None;
        }

        Some(line_start + byte_in_line)
    }

    /// Переводит байтовый offset Rope в LSP-позицию (line, character в UTF-16)
    pub fn from_byte_offset(rope: &Rope, byte_offset: usize) -> Option<Self> {
        if byte_offset > rope.len() {
            return None;
        }
        let line = rope.line_of_offset(byte_offset);
        let line_start = rope.offset_of_line(line);
        let offset_in_line = byte_offset - line_start;

        let line_end = rope.offset_of_line(line + 1).min(rope.len());
        let line_text = rope.slice(line_start..line_end).to_string();

        let mut acc_bytes = 0;
        let mut utf16_col = 0;

        for ch in line_text.chars() {
            let ch_bytes = ch.len_utf8();
            if acc_bytes + ch_bytes > offset_in_line {
                break;
            }
            acc_bytes += ch_bytes;
            utf16_col += ch.len_utf16();
        }

        // если мы в середине символа — невалидно
        if acc_bytes != offset_in_line {
            return None;
        }

        Some(Self {
            line,
            character: utf16_col,
        })
    }

    /// Переводит LSP-позицию в tree-sitter `Point` (row — строка, column — **байты** внутри строки).
    pub fn to_point(&self, rope: &Rope) -> Option<Point> {
        let byte = self.to_byte_offset(rope)?;
        let line_start = rope.offset_of_line(self.line);
        Some(Point {
            row: self.line,
            column: byte - line_start,
        })
    }
}
