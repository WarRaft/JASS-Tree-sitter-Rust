use crate::lsp::position::Position;

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
            offset += self.lines.get(i)?.len() + 1; // +1 за '\n'
        }
        Some(offset + pos.character)
    }

    pub fn point_from_offset(&self, offset: usize) -> tree_sitter::Point {
        let mut total = 0;
        for (row, line) in self.lines.iter().enumerate() {
            let len = line.len() + 1;
            if total + len > offset {
                return tree_sitter::Point {
                    row,
                    column: offset - total,
                };
            }
            total += len;
        }
        // Если offset за концом текста
        let last_row = self.lines.len().saturating_sub(1);
        let last_col = self.lines.get(last_row).map_or(0, |l| l.len());
        tree_sitter::Point {
            row: last_row,
            column: last_col,
        }
    }

    pub fn apply_change(&mut self, start: &Position, end: &Position, new_text: &str) {
        let start_line = start.line;
        let start_col = start.character;
        let end_line = end.line;
        let end_col = end.character;

        let before = &self.lines[start_line][..start_col];
        let after = &self.lines[end_line][end_col..];

        let new_lines: Vec<String> = new_text.lines().map(|s| s.to_string()).collect();
        let replacement = match new_lines.len() {
            0 => vec![format!("{before}{after}")],
            1 => vec![format!("{before}{}{}", new_lines[0], after)],
            _ => {
                let mut result = Vec::new();
                result.push(format!("{before}{}", new_lines[0]));
                result.extend_from_slice(&new_lines[1..new_lines.len() - 1]);
                result.push(format!("{}{}", new_lines.last().unwrap(), after));
                result
            }
        };

        self.lines.splice(start_line..=end_line, replacement);
    }
}
