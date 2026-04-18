use super::test_support::*;
use crate::http::semantic::token::Kind as TokenKind;

    #[test]
    fn semantic_tokens_present() {
        let src = "\
function Foo takes nothing returns nothing
    call Bar()
endfunction
";
        with_cursor(src, |c| {
            let data = c.semantic.data(None);
            assert!(!data.is_empty(), "Should have semantic tokens");
        });
    }

    #[test]
    fn semantic_function_call_name_is_function() {
        let src = "call Foo()\n";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            let foo_token = line.tokens.iter().find(|t| t.col == 5 && t.len == 3);
            assert!(
                foo_token.is_some(),
                "Should have a token for 'Foo' at col=5 len=3, tokens: {:?}",
                line.tokens
            );
            assert_eq!(
                foo_token.unwrap().kind,
                TokenKind::Function,
                "Function call name should be TokenKind::Function, got {:?}",
                foo_token.unwrap().kind
            );
        });
    }

    #[test]
    fn semantic_function_call_inside_body_is_function() {
        let src = "\
function main takes nothing returns nothing
    call Foo()
endfunction
";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&1).expect("should have line 1");
            eprintln!("line 1 tokens: {:?}", line.tokens);
            let tok = line.tokens.iter().find(|t| t.col == 9 && t.len == 3);
            assert!(
                tok.is_some(),
                "Should have token for 'Foo' at col=9 len=3, tokens: {:?}",
                line.tokens
            );
            assert_eq!(
                tok.unwrap().kind,
                TokenKind::Function,
                "call Foo() name should be Function, got {:?}",
                tok.unwrap().kind
            );
        });
    }

    #[test]
    fn semantic_function_decl_name_is_function() {
        let src = "function MyFunc takes nothing returns nothing\nendfunction\n";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            let tok = line.tokens.iter().find(|t| t.col == 9 && t.len == 6);
            assert!(
                tok.is_some(),
                "Should have token for 'MyFunc', tokens: {:?}",
                line.tokens
            );
            assert_eq!(tok.unwrap().kind, TokenKind::Function);
        });
    }

    #[test]
    fn semantic_type_name_is_type() {
        let src = "type handle extends agent\n";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            let tok = line.tokens.iter().find(|t| t.col == 5 && t.len == 6);
            assert!(
                tok.is_some(),
                "Should have token for 'handle', tokens: {:?}",
                line.tokens
            );
            assert_eq!(tok.unwrap().kind, TokenKind::Type);
            let tok2 = line.tokens.iter().find(|t| t.col == 20 && t.len == 5);
            assert!(
                tok2.is_some(),
                "Should have token for 'agent', tokens: {:?}",
                line.tokens
            );
            assert_eq!(tok2.unwrap().kind, TokenKind::Type);
        });
    }

    #[test]
    fn semantic_string_literal() {
        let src = "call Foo(\"my shit\")\n";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            // String literal is tokenized into sub-ranges (quotes + content).
            // All tokens covering cols 9..18 should be String.
            let str_tokens: Vec<_> = line.tokens.iter()
                .filter(|t| t.col >= 9 && t.col < 18)
                .collect();
            assert!(
                !str_tokens.is_empty(),
                "Should have string tokens at cols 9..18, tokens: {:?}",
                line.tokens
            );
            for tok in &str_tokens {
                assert_eq!(
                    tok.kind,
                    TokenKind::String,
                    "String literal sub-token should be TokenKind::String, got {:?}",
                    tok.kind
                );
            }
            // Total length of all string tokens should be 9
            let total_len: usize = str_tokens.iter().map(|t| t.len).sum();
            assert_eq!(total_len, 9, "Total string token length should be 9 (including quotes)");
        });
    }

    #[test]
    fn semantic_function_call_in_expression() {
        let src = "\
function F takes unit Target returns boolean
    return UnitLife(Target) > 0 and not IsHidden(Target)
endfunction
";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&1).expect("should have line 1");
            let ul_tok = line.tokens.iter().find(|t| t.col == 11 && t.len == 8);
            assert!(
                ul_tok.is_some(),
                "Should have UnitLife token, tokens: {:?}",
                line.tokens
            );
            assert_eq!(
                ul_tok.unwrap().kind,
                TokenKind::Function,
                "UnitLife should be Function, got {:?}",
                ul_tok.unwrap().kind
            );
            let ih_tok = line.tokens.iter().find(|t| t.col == 40 && t.len == 8);
            assert!(
                ih_tok.is_some(),
                "Should have IsHidden token, tokens: {:?}",
                line.tokens
            );
            assert_eq!(
                ih_tok.unwrap().kind,
                TokenKind::Function,
                "IsHidden should be Function, got {:?}",
                ih_tok.unwrap().kind
            );
        });
    }

    #[test]
    fn semantic_no_trailing_newline() {
        // File ends with "endglobals" right at EOF, no \n
        let src = "globals\n    integer a = 0xFF23A284\nendglobals";
        with_cursor(src, |c| {
            let data = c.semantic.data(None);
            assert!(
                !data.is_empty(),
                "Should have semantic tokens even without trailing newline"
            );
            // "integer" on line 1 should be a Type token
            let line1 = c.semantic.lines.get(&1);
            assert!(
                line1.is_some(),
                "Should have tokens on line 1, all lines: {:?}",
                c.semantic.lines.keys().collect::<Vec<_>>()
            );
            let int_tok = line1
                .unwrap()
                .tokens
                .iter()
                .find(|t| t.col == 4 && t.len == 7);
            assert!(
                int_tok.is_some(),
                "Should have 'integer' token at col=4 len=7, tokens: {:?}",
                line1.unwrap().tokens
            );
            assert_eq!(int_tok.unwrap().kind, TokenKind::Type);
        });
    }

    #[test]
    fn semantic_no_trailing_newline_single_line() {
        // Single line, no newline — everything on line 0
        let src = "globals\ninteger a = 0xFF23A284\nendglobals";
        with_cursor(src, |c| {
            eprintln!(
                "All semantic lines: {:?}",
                c.semantic.lines.keys().collect::<Vec<_>>()
            );
            for (line_num, line) in &c.semantic.lines {
                eprintln!("  line {}: {:?}", line_num, line.tokens);
            }
            // "integer" should be on line 1
            let line1 = c.semantic.lines.get(&1);
            assert!(line1.is_some(), "Should have tokens on line 1");
        });
    }

    #[test]
    fn semantic_eof_after_number() {
        // endglobals at EOF without trailing newline
        let src = "globals\n    integer a = 0xFF23A284\nendglobals";
        with_cursor(src, |c| {
            let data = c.semantic.data(None);
            assert!(
                !data.is_empty(),
                "Should have semantic tokens even without trailing newline"
            );
            // "integer" on line 1 should be a Type token
            let line1 = c.semantic.lines.get(&1);
            assert!(line1.is_some(), "Should have tokens on line 1");
            let int_tok = line1
                .unwrap()
                .tokens
                .iter()
                .find(|t| t.col == 4 && t.len == 7);
            assert!(int_tok.is_some(), "Should have 'integer' token");
            assert_eq!(int_tok.unwrap().kind, TokenKind::Type);
            // endglobals keyword on line 2
            let line2 = c.semantic.lines.get(&2);
            assert!(
                line2.is_some(),
                "Should have tokens on line 2 (endglobals at EOF), all lines: {:?}",
                c.semantic.lines.keys().collect::<Vec<_>>()
            );
        });
    }
