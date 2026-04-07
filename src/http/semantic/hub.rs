use crate::http::semantic::token::Kind;
use crate::lsp::range::Range;
use lapce_xi_rope::Rope;
use std::collections::BTreeMap;
use tree_sitter::Node;

/// A single semantic token on a line.
///
/// Positions are in **UTF-16 code units** (what VS Code calls "characters").
#[derive(Debug, Clone)]
pub struct Token {
    /// 0-based line number.
    pub row: usize,
    /// 0-based column (UTF-16 code units from line start).
    pub col: usize,
    /// Length in UTF-16 code units.
    pub len: usize,
    /// Semantic token type (namespace, function, variable, …).
    pub kind: Kind,
    /// Bitmask of semantic token modifiers (declaration, readonly, …).
    pub modifiers: u32,
}

/// All semantic tokens on a single line.
#[derive(Debug)]
pub struct Line {
    /// 0-based line number.
    pub index: usize,
    /// Tokens on this line (unsorted — sorted lazily in [`Hub::data`]).
    pub tokens: Vec<Token>,
}

impl Line {
    /// Create an empty line entry for the given 0-based line number.
    pub fn new(index: usize) -> Self {
        Self {
            index,
            tokens: Vec::new(),
        }
    }

    /// Append a token to this line.
    pub fn add(&mut self, token: Token) {
        self.tokens.push(token);
    }
}

/// Collector for semantic tokens produced during a parse.
///
/// Language-specific cursors call [`add_node`](Self::add_node) /
/// [`add_range`](Self::add_range) while walking the tree, then
/// [`data`](Self::data) serialises everything into the delta-encoded
/// `[Δline, Δstart, length, tokenType, tokenModifiers]` array that
/// VS Code expects.
#[derive(Default, Debug)]
pub struct Hub {
    /// Tokens grouped by 0-based line number.
    pub lines: BTreeMap<usize, Line>,
}

impl Hub {
    /// Register a semantic token that corresponds to a tree-sitter node.
    ///
    /// Multi-line nodes are split into one token per line.  Positions
    /// are converted from byte offsets to UTF-16 columns so they match
    /// VS Code's internal representation.
    ///
    /// Zero-length or negative-length nodes (MISSING / error recovery
    /// artefacts) are silently skipped.
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

            // VS Code measures line length in UTF-16 code units *excluding*
            // the line ending (\n or \r\n).  We must cap byte positions at
            // the content boundary so that `col + len` never exceeds it.
            let content_len = line_text
                .trim_end_matches(|c: char| c == '\n' || c == '\r')
                .len();

            // Relative byte boundaries within the current line,
            // clamped to the visible content (no newline bytes).
            let rel_start = if row == start_line {
                start_byte
                    .saturating_sub(line_start_byte)
                    .min(content_len)
            } else {
                0
            };
            let rel_end = if row == end_line {
                end_byte
                    .saturating_sub(line_start_byte)
                    .min(content_len)
            } else {
                content_len
            };

            // Convert byte positions to UTF-16 column offsets.
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

    /// Register a semantic token from raw byte offset + byte length.
    ///
    /// Useful when the token doesn't correspond to a single tree-sitter
    /// node (e.g. sub-ranges of a comment node used as an import
    /// directive).  Behaves identically to [`add_node`](Self::add_node)
    /// but takes explicit byte boundaries instead of a `Node`.
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

            // Cap at content boundary (exclude trailing \n / \r\n) —
            // same guard as in add_node.
            let content_len = line_text
                .trim_end_matches(|c: char| c == '\n' || c == '\r')
                .len();

            let rel_start = if row == start_line {
                start_byte
                    .saturating_sub(line_start_byte)
                    .min(content_len)
            } else {
                0
            };
            let rel_end = if row == end_line {
                end_byte
                    .saturating_sub(line_start_byte)
                    .min(content_len)
            } else {
                content_len
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

    /// Produce the delta-encoded semantic token array.
    ///
    /// Each token is 5 consecutive `u32` values:
    /// `[Δline, Δstart, length, tokenType, tokenModifiers]`
    ///
    /// The encoding matches the VS Code `SemanticTokens.data` format
    /// and is directly usable as a `Uint32Array` on the JS side —
    /// for the binary TLV wire protocol (`SECTION_SEMANTIC` / `SECTION_SEMANTIC_EDIT`).
    ///
    /// When `range` is `Some`, only tokens intersecting the given line/
    /// character range are emitted (used by `semanticTokens/range`).
    pub fn data(&self, range: Option<Range>) -> Vec<u32> {
        let mut result = Vec::new();
        let mut line_last: u32 = 0;

        let mut lines: Vec<_> = self.lines.values().collect();
        lines.sort_by_key(|line| line.index);

        for line in lines {
            let line_index = line.index as u32;

            if let Some(ref range) = range {
                let start = range.start.line as u32;
                let end = range.end.line as u32;
                if line_index < start || line_index > end {
                    continue;
                }
            }

            let mut tokens: Vec<_> = line.tokens.iter().collect();
            tokens.sort_by_key(|t| t.col);

            let mut token_last: u32 = 0;
            let mut any_token = false;

            for token in tokens {
                let token_row = token.row as u32;
                let token_col = token.col as u32;

                if let Some(ref range) = range {
                    let rs = range.start.line as u32;
                    let re = range.end.line as u32;
                    let cs = range.start.character as u32;
                    let ce = range.end.character as u32;
                    if token_row < rs || token_row > re {
                        continue;
                    }
                    if token_row == rs && token_col < cs {
                        continue;
                    }
                    if token_row == re && token_col >= ce {
                        continue;
                    }
                }

                let delta_line = if !any_token {
                    token_row.saturating_sub(line_last)
                } else {
                    0
                };

                let delta_start = token_col.saturating_sub(token_last);

                result.push(delta_line);
                result.push(delta_start);
                result.push(token.len as u32);
                result.push(token.kind as u32);
                result.push(token.modifiers);

                token_last = token_col;
                any_token = true;
            }

            if any_token {
                line_last = line_index;
            }
        }

        result
    }
}
