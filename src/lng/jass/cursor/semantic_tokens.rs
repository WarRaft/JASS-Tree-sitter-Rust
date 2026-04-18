//! Semantic token builder — CST DFS walk that assigns semantic highlighting tokens.
//!
//! Extracted from `mod.rs`.

use crate::http::semantic::token::Kind as TokenKind;
use crate::lng::jass::ast::IdRole;
use crate::lng::jass::kind::Kind;
use tree_sitter::Node;

use super::Cursor;

impl Cursor {
    // ─── Semantic tokens from CST DFS ────────────────────────────────────

    pub(super) fn build_semantic(&mut self, root: &Node) {
        let mut cursor = root.walk();
        let mut visit = true;
        loop {
            if visit {
                let node = cursor.node();
                let kind = Kind::try_from(node.kind_id()).ok();

                // Import directive comment nodes are handled in the AST pass —
                // skip them entirely so they don't get re-coloured as Comment.
                if kind == Some(Kind::Comment) && self.directive_nodes.contains(&node.start_byte()) {
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    while !cursor.goto_next_sibling() {
                        if !cursor.goto_parent() {
                            return;
                        }
                    }
                    continue;
                }

                // String literal: tokenize with escape/color-code awareness
                if kind == Some(Kind::StringLiteral) {
                    crate::lng::string_colors::tokenize_string_literal(&node, &self.rope, &mut self.semantic);
                    self.colors.extend(crate::lng::string_colors::collect_string_colors(&node, &self.rope));
                    // Don't descend into children (quotes, content)
                    if cursor.goto_next_sibling() {
                        continue;
                    }
                    // Go up
                    while !cursor.goto_next_sibling() {
                        if !cursor.goto_parent() {
                            return;
                        }
                    }
                    continue;
                }

                // Only leaf nodes get semantic tokens
                if node.child_count() == 0 {
                    if let Some(kind) = Kind::try_from(node.grammar_id()).ok() {
                        let token_kind = match kind {
                            Kind::Id => {
                                if let Some(&role) = self.id_roles.get(&node.start_byte()) {
                                    match role {
                                        IdRole::FunctionDecl | IdRole::FunctionRef => TokenKind::Function,
                                        IdRole::TypeDecl | IdRole::TypeRef => TokenKind::Type,
                                        IdRole::Param => TokenKind::Parameter,
                                        IdRole::Variable | IdRole::Constant => TokenKind::Variable,
                                    }
                                } else {
                                    TokenKind::Variable
                                }
                            }
                            Kind::Function | Kind::Endfunction | Kind::Native | Kind::Type
                            | Kind::Extends | Kind::Takes | Kind::Returns | Kind::Nothing
                            | Kind::Local | Kind::Set | Kind::Call | Kind::Return
                            | Kind::If | Kind::Then | Kind::Elseif | Kind::Else | Kind::Endif
                            | Kind::Loop | Kind::Endloop | Kind::Exitwhen
                            | Kind::Globals | Kind::Endglobals
                            | Kind::Constant | Kind::Array => TokenKind::Keyword,

                            Kind::And | Kind::Or | Kind::Not
                            | Kind::Equal | Kind::Comma
                            | Kind::LeftParen | Kind::RightParen
                            | Kind::LeftBracket | Kind::RightBracket
                            | Kind::Plus | Kind::Minus | Kind::Star | Kind::Slash
                            | Kind::PlusPlus | Kind::MinusMinus
                            | Kind::Lt | Kind::Gt | Kind::Le | Kind::Ge
                            | Kind::EqEq | Kind::Neq => TokenKind::Operator,

                            Kind::Number | Kind::Float | Kind::Rawcode => {
                                // Collect color from hex literals like 0xAARRGGBB
                                if kind == Kind::Number {
                                    if let Some(ci) = crate::lng::string_colors::collect_hex_literal_color(&node, &self.rope) {
                                        self.colors.push(ci);
                                    }
                                }
                                TokenKind::Number
                            }
                            Kind::Comment => {
                                // //* doc comment and //@ignore: prefix as Comment, body as String
                                let sb = node.start_byte();
                                let eb = node.end_byte();
                                let text = self.rope.slice_to_cow(sb..eb);
                                let trimmed = text.trim_start();
                                if trimmed.starts_with("//*") {
                                    let prefix_len = 3; // "//*"
                                    let ws_before = text.len() - trimmed.len();
                                    self.semantic.add_range(sb + ws_before, prefix_len, &self.rope, TokenKind::Comment, 0u32);
                                    let rest_start = sb + ws_before + prefix_len;
                                    if rest_start < eb {
                                        self.semantic.add_range(rest_start, eb - rest_start, &self.rope, TokenKind::String, 0u32);
                                    }
                                    // skip the default add_node below
                                    if cursor.goto_next_sibling() { continue; }
                                    while !cursor.goto_next_sibling() {
                                        if !cursor.goto_parent() { return; }
                                    }
                                    continue;
                                } else if trimmed.starts_with("//@ignore") {
                                    let prefix_len = "//@ignore".len();
                                    let ws_before = text.len() - trimmed.len();
                                    let abs_prefix = sb + ws_before;
                                    // Macro token for the "//@ignore" prefix (same as //set)
                                    self.semantic.add_range(abs_prefix, prefix_len, &self.rope, TokenKind::Macro, 0u32);
                                    // Each tag word as Property token (same as //set key)
                                    let after = &trimmed[prefix_len..];
                                    let mut byte_off = 0usize;
                                    for word in after.split_whitespace() {
                                        // find word start relative to `after`
                                        let wstart = after[byte_off..].find(word).unwrap() + byte_off;
                                        let abs_pos = abs_prefix + prefix_len + wstart;
                                        self.semantic.add_range(abs_pos, word.len(), &self.rope, TokenKind::Property, 0u32);
                                        byte_off = wstart + word.len();
                                    }
                                    if cursor.goto_next_sibling() { continue; }
                                    while !cursor.goto_next_sibling() {
                                        if !cursor.goto_parent() { return; }
                                    }
                                    continue;
                                }
                                TokenKind::Comment
                            }
                            _ => {
                                // Descend
                                if cursor.goto_first_child() { continue; }
                                #[allow(unused_assignments)]
                                { visit = false; }
                                if cursor.goto_next_sibling() { visit = true; continue; }
                                while !cursor.goto_next_sibling() {
                                    if !cursor.goto_parent() { return; }
                                }
                                visit = true;
                                continue;
                            }
                        };
                        self.semantic.add_node(&node, &self.rope, token_kind, 0u32);
                    }
                }
            }

            // DFS traversal
            if visit && cursor.goto_first_child() {
                continue;
            }
            visit = true;
            if cursor.goto_next_sibling() {
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    return;
                }
            }
        }
    }
}

