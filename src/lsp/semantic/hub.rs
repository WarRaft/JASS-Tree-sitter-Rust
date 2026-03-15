use crate::lsp::range::Range;
use crate::lsp::semantic::lsp::Kind;
use lapce_xi_rope::Rope;
use std::collections::BTreeMap;
use tree_sitter::Node;

#[derive(Debug, Clone)]
pub struct Token {
    pub row: usize,
    pub col: usize,
    pub len: usize,
    pub kind: Kind,
    pub modifiers: u32,
}

#[derive(Debug)]
pub struct Line {
    pub index: usize,
    pub tokens: Vec<Token>,
}

impl Line {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            tokens: Vec::new(),
        }
    }

    pub fn add(&mut self, token: Token) {
        self.tokens.push(token);
    }
}

#[derive(Default, Debug)]
pub struct Hub {
    pub lines: BTreeMap<usize, Line>,
}

impl Hub {
    pub fn add_node(
        &mut self,
        node: &Node,
        rope: &Rope,
        kind: Kind,
        modifiers: impl Into<u32>,
    ) -> &mut Self {
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();

        // Zero-length nodes (MISSING / error nodes from tree-sitter during
        // incremental editing) — silently skip, nothing to highlight.
        if end_byte <= start_byte {
            return self;
        }

        let modifiers = modifiers.into();

        let start_line = rope.line_of_offset(start_byte);
        let end_line = rope.line_of_offset(end_byte);

        for row in start_line..=end_line {
            let line_start_byte = rope.offset_of_line(row);
            let line_end_byte = rope.offset_of_line(row + 1).min(rope.len());
            let line_text = rope.slice(line_start_byte..line_end_byte).to_string();

            // Относительные байтовые границы для текущей строки,
            // но ограничиваем в пределах строки
            let rel_start = if row == start_line {
                start_byte
                    .saturating_sub(line_start_byte)
                    .min(line_text.len())
            } else {
                0
            };
            let rel_end = if row == end_line {
                end_byte
                    .saturating_sub(line_start_byte)
                    .min(line_text.len())
            } else {
                line_text.len()
            };

            // Считаем UTF-16 колонки для начала и конца токена в этой строке
            let utf16_start_col = line_text[..rel_start].encode_utf16().count();
            let utf16_end_col = line_text[..rel_end].encode_utf16().count();

            let len = utf16_end_col.saturating_sub(utf16_start_col);
            if len == 0 {
                continue;
            }

            self.lines
                .entry(row)
                .or_insert_with(|| Line::new(row))
                .add(Token {
                    row,
                    col: utf16_start_col,
                    len,
                    kind,
                    modifiers,
                });
        }

        self
    }

    /// Emit a semantic token from raw byte offset + byte length.
    ///
    /// Useful when the token doesn't correspond to a single tree-sitter node
    /// (e.g. sub-ranges of a comment node used as an import directive).
    pub fn add_range(
        &mut self,
        start_byte: usize,
        byte_len: usize,
        rope: &Rope,
        kind: Kind,
        modifiers: impl Into<u32>,
    ) -> &mut Self {
        if byte_len == 0 {
            return self;
        }
        let end_byte = start_byte + byte_len;
        let modifiers = modifiers.into();

        let start_line = rope.line_of_offset(start_byte);
        let end_line = rope.line_of_offset(end_byte);

        for row in start_line..=end_line {
            let line_start_byte = rope.offset_of_line(row);
            let line_end_byte = rope.offset_of_line(row + 1).min(rope.len());
            let line_text = rope.slice(line_start_byte..line_end_byte).to_string();

            let rel_start = if row == start_line {
                start_byte
                    .saturating_sub(line_start_byte)
                    .min(line_text.len())
            } else {
                0
            };
            let rel_end = if row == end_line {
                end_byte
                    .saturating_sub(line_start_byte)
                    .min(line_text.len())
            } else {
                line_text.len()
            };

            let utf16_start_col = line_text[..rel_start].encode_utf16().count();
            let utf16_end_col = line_text[..rel_end].encode_utf16().count();

            let len = utf16_end_col.saturating_sub(utf16_start_col);
            if len == 0 {
                continue;
            }

            self.lines
                .entry(row)
                .or_insert_with(|| Line::new(row))
                .add(Token {
                    row,
                    col: utf16_start_col,
                    len,
                    kind,
                    modifiers,
                });
        }

        self
    }

    /// Adjust token column positions on specific lines.
    ///
    /// `deltas` maps **0-based line number** → signed column shift.
    /// Positive values shift tokens to the right (indent increased);
    /// negative values shift left (indent decreased).
    ///
    /// This is used after formatting: leading-whitespace edits change
    /// the column of every token on the affected line by the same
    /// amount.  Token `len` and `row` stay unchanged.
    pub fn adjust_columns(&mut self, deltas: &std::collections::HashMap<usize, isize>) {
        for (&line, line_data) in self.lines.iter_mut() {
            if let Some(&delta) = deltas.get(&line) {
                for token in &mut line_data.tokens {
                    let new_col = token.col as isize + delta;
                    token.col = new_col.max(0) as usize;
                }
            }
        }
    }

    pub fn data(&self, range: Option<Range>) -> Vec<usize> {
        let mut result = Vec::new();
        let mut line_last = 0;

        let mut lines: Vec<_> = self.lines.values().collect();
        lines.sort_by_key(|line| line.index);

        for line in lines {
            let line_index = line.index;

            if let Some(ref range) = range {
                if line_index < range.start.line || line_index > range.end.line {
                    continue;
                }
            }

            let mut tokens: Vec<_> = line.tokens.iter().collect();
            tokens.sort_by_key(|t| t.col);

            let mut token_last = 0;
            let mut any_token = false;

            for token in tokens {
                if let Some(ref range) = range {
                    if token.row < range.start.line || token.row > range.end.line {
                        continue;
                    }
                    if token.row == range.start.line && token.col < range.start.character {
                        continue;
                    }
                    if token.row == range.end.line && token.col >= range.end.character {
                        continue;
                    }
                }

                let delta_line = if !any_token {
                    token.row.saturating_sub(line_last)
                } else {
                    0
                };

                let delta_start = token.col.saturating_sub(token_last);

                result.push(delta_line);
                result.push(delta_start);
                result.push(token.len);
                result.push(token.kind as usize);
                result.push(token.modifiers as usize);

                token_last = token.col;
                any_token = true;
            }

            if any_token {
                line_last = line_index;
            }
        }

        result
    }
}
