use crate::lsp::semantic::{TokenModifier, TokenType};
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct SemanticToken {
    pub line: u32,
    pub pos: u32,
    pub len: u32,
    pub token_type: TokenType,
    pub modifier: Option<TokenModifier>,
}

pub struct SemanticTokenLine {
    pub index: u32,
    pub tokens: Vec<SemanticToken>,
}

impl SemanticTokenLine {
    pub fn new(index: u32) -> Self {
        Self {
            index,
            tokens: Vec::new(),
        }
    }

    pub fn add(&mut self, token: SemanticToken) {
        self.tokens.push(token);
    }
}

pub struct SemanticTokenHub {
    pub lines: BTreeMap<u32, SemanticTokenLine>,
}

impl SemanticTokenHub {
    pub fn new() -> Self {
        Self {
            lines: BTreeMap::new(),
        }
    }

    pub fn add(
        &mut self,
        line: u32,
        pos: u32,
        len: u32,
        token_type: TokenType,
        modifier: Option<TokenModifier>,
    ) -> &mut Self {
        let token = SemanticToken {
            line,
            pos,
            len,
            token_type,
            modifier,
        };

        self.lines
            .entry(line)
            .or_insert_with(|| SemanticTokenLine::new(line))
            .add(token);

        self
    }
    pub fn data(&self) -> Vec<u32> {
        let mut result = Vec::new();
        let mut line_last = 0;

        for line in self.lines.values() {
            let mut tokens = line.tokens.clone();
            tokens.sort_by_key(|t| t.pos);
            let mut token_last = 0;

            for (i, token) in tokens.iter().enumerate() {
                result.push(if i == 0 { token.line - line_last } else { 0 });
                result.push(token.pos - token_last);
                result.push(token.len);
                result.push(token.token_type.clone() as u32);
                result.push(token.modifier.as_ref().map_or(0, |m| m.clone() as u32));
                token_last = token.pos;
            }

            line_last = line.index;
        }

        result
    }
    pub fn clear(&mut self) {
        self.lines.clear();
    }
}
