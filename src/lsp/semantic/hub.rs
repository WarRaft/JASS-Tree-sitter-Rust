use std::collections::BTreeMap;
use tree_sitter::Node;
use crate::lsp::range::Range;
use crate::lsp::semantic::lsp::Kind;

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

#[derive(Debug)]
pub struct Hub {
    pub lines: BTreeMap<usize, Line>,
}

impl Hub {
    pub fn new() -> Self {
        Self {
            lines: BTreeMap::new(),
        }
    }

    pub fn add_node(
        &mut self,
        node: &Node,
        token_type: Kind,
        modifiers: impl Into<u32>,
    ) -> &mut Self {
        let s = node.start_position();
        let e = node.end_position();

        if s.row != e.row {
            return self;
        }

        self.lines
            .entry(s.row)
            .or_insert_with(|| Line::new(s.row))
            .add(Token {
                row: s.row,
                col: s.column,
                len: e.column.saturating_sub(s.column),
                kind: token_type,
                modifiers: modifiers.into(),
            });
        self
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



    pub fn clear(&mut self) -> &mut Self {
        self.lines.clear();
        self
    }
}
