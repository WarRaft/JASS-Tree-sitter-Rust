use crate::lsp::position::Position;
use tree_sitter::Point;

#[derive(Clone, Debug)]
pub struct LineList {
    pub lines: Vec<String>,
}

impl LineList {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    pub fn set_text(&mut self, text: impl AsRef<[u8]>) {
        let text = std::str::from_utf8(text.as_ref()).expect("Invalid UTF-8");
        self.lines = text.lines().map(str::to_string).collect();
    }

    pub fn to_text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn position_to_offset(&self, pos: &Position) -> Option<usize> {
        if pos.line >= self.lines.len() {
            return None;
        }

        let mut offset = 0;

        for i in 0..pos.line {
            offset += self.lines.get(i)?.as_bytes().len() + 1; // +1 за '\n'
        }

        let line = self.lines.get(pos.line)?;
        let byte_offset_in_line = line
            .chars()
            .take(pos.character)
            .map(|c| c.len_utf8())
            .sum::<usize>();

        Some(offset + byte_offset_in_line)
    }

    pub fn point_from_offset(&self, offset: usize) -> Point {
        let mut total = 0;

        for (row, line) in self.lines.iter().enumerate() {
            let line_len = line.as_bytes().len() + 1; // +1 за '\n'
            if total + line_len > offset {
                let column = offset - total;
                return Point { row, column };
            }
            total += line_len;
        }

        let last_row = self.lines.len().saturating_sub(1);
        let last_col = self.lines.get(last_row).map_or(0, |l| l.as_bytes().len());
        Point {
            row: last_row,
            column: last_col,
        }
    }

    pub fn apply_change(&mut self, start: &Position, end: &Position, new_text: &str) {
        let start_line = start.line;
        let end_line = end.line;

        let start_col = {
            let line = &self.lines[start_line];
            line.chars()
                .take(start.character)
                .map(|c| c.len_utf8())
                .sum::<usize>()
        };

        let end_col = {
            let line = &self.lines[end_line];
            line.chars()
                .take(end.character)
                .map(|c| c.len_utf8())
                .sum::<usize>()
        };

        let before = &self.lines[start_line][..start_col];
        let after = &self.lines[end_line][end_col..];

        let new_lines: Vec<String> = new_text.lines().map(str::to_string).collect();
        let replacement = match new_lines.len() {
            0 => vec![format!("{before}{after}")],
            1 => vec![format!("{before}{}{}", new_lines[0], after)],
            _ => {
                let mut result = Vec::with_capacity(new_lines.len());
                result.push(format!("{before}{}", new_lines[0]));
                result.extend_from_slice(&new_lines[1..new_lines.len() - 1]);
                result.push(format!("{}{}", new_lines.last().unwrap(), after));
                result
            }
        };

        self.lines.splice(start_line..=end_line, replacement);
    }
}
