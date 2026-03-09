#[cfg(test)]
mod tests {
    use crate::lng::ass::ast::*;
    use crate::lng::ass::cursor::Cursor;
    use crate::lsp::semantic::lsp::Kind as TokenKind;
    use lapce_xi_rope::Rope;

    fn with_cursor(src: &str, f: impl FnOnce(&Cursor)) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_as::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        let rope = Rope::from(src);
        let cursor = Cursor::walk(&ast, &rope);
        f(&cursor);
    }

    fn collect_tokens(src: &str, cursor: &Cursor) -> Vec<(String, TokenKind)> {
        let mut result = Vec::new();
        for (_line_idx, line) in &cursor.semantic.lines {
            for token in &line.tokens {
                let text: String = src.lines()
                    .nth(token.row)
                    .map(|l| l.chars().skip(token.col).take(token.len).collect())
                    .unwrap_or_default();
                result.push((text, token.kind));
            }
        }
        result
    }

    #[test]
    fn local_var_type_is_type_not_variable() {
        let src = "\
int CountUnitInGroupOfPlayer(player p, int id) {
    group g = CreateGroup();
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let group_token = tokens.iter().find(|(t, _)| t == "group").unwrap();
            assert_eq!(group_token.1, TokenKind::Type,
                "Expected 'group' to be Type, got {:?}", group_token.1);

            let cg_token = tokens.iter().find(|(t, _)| t == "CreateGroup").unwrap();
            assert_eq!(cg_token.1, TokenKind::Function,
                "Expected 'CreateGroup' to be Function, got {:?}", cg_token.1);

            let p_token = tokens.iter().find(|(t, _)| t == "p").unwrap();
            assert_eq!(p_token.1, TokenKind::Parameter,
                "Expected 'p' to be Parameter, got {:?}", p_token.1);

            let g_token = tokens.iter().find(|(t, _)| t == "g").unwrap();
            assert_eq!(g_token.1, TokenKind::Variable,
                "Expected 'g' to be Variable, got {:?}", g_token.1);

            let player_token = tokens.iter().find(|(t, _)| t == "player").unwrap();
            assert_eq!(player_token.1, TokenKind::Type,
                "Expected 'player' to be Type, got {:?}", player_token.1);
        });
    }
}

