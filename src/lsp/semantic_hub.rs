use crate::lsp::semantic::{TokenModifier, TokenType};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Token {
    pub line: usize,
    pub pos: usize,
    pub len: usize,
    pub token_type: TokenType,
    pub modifier: Option<TokenModifier>,
}

#[derive(Debug)]
pub struct TokenLine {
    pub index: usize,
    pub tokens: Vec<Token>,
}

impl TokenLine {
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
pub struct SemanticTokenHub {
    pub lines: BTreeMap<usize, TokenLine>,
}

impl SemanticTokenHub {
    pub fn new() -> Self {
        Self {
            lines: BTreeMap::new(),
        }
    }

    pub fn add(
        &mut self,
        line: usize,
        pos: usize,
        len: usize,
        token_type: TokenType,
        modifier: Option<TokenModifier>,
    ) -> &mut Self {
        self.lines
            .entry(line)
            .or_insert_with(|| TokenLine::new(line))
            .add(Token {
                line,
                pos,
                len,
                token_type,
                modifier,
            });

        self
    }
    pub fn data(&self) -> Vec<usize> {
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
                result.push(token.token_type.clone() as usize);
                result.push(token.modifier.as_ref().map_or(0, |m| m.clone() as usize));
                token_last = token.pos;
            }

            line_last = line.index;
        }

        result
    }
    pub fn clear(&mut self) -> &mut Self {
        self.lines.clear();
        self
    }
}
