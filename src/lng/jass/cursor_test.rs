#[cfg(test)]
mod tests {
    use crate::lng::jass::ast::*;
    use crate::lng::jass::cursor::Cursor;
    use crate::lsp::document_symbol::lsp::SymbolKind;
    use crate::lsp::folding::lsp::FoldingRangeKind;
    use crate::lsp::semantic::lsp::Kind as TokenKind;
    use lapce_xi_rope::Rope;

    fn with_cursor(src: &str, f: impl FnOnce(&Cursor)) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        let rope = Rope::from(src);
        let cursor = Cursor::walk(&ast, &rope);
        f(&cursor);
    }

    #[test]
    fn symbols_function() {
        let src = "\
function Foo takes integer x returns nothing
    local integer y = 1
endfunction
";
        with_cursor(src, |c| {
            assert_eq!(c.symbols.len(), 1);
            assert_eq!(c.symbols[0].name, "Foo");
            let ch = c.symbols[0].children.as_ref().unwrap();
            assert_eq!(ch.len(), 2);
            assert_eq!(ch[0].name, "x");
            assert_eq!(ch[1].name, "y");
        });
    }

    #[test]
    fn symbols_globals() {
        let src = "\
globals
    constant integer MAX = 100
    real x
endglobals
";
        with_cursor(src, |c| {
            assert_eq!(c.symbols.len(), 1);
            let ch = c.symbols[0].children.as_ref().unwrap();
            assert_eq!(ch[0].kind, SymbolKind::Constant);
            assert_eq!(ch[1].kind, SymbolKind::Variable);
        });
    }

    #[test]
    fn folding_regions() {
        let src = "\
function F takes nothing returns nothing
    if true then
        return
    endif
endfunction
";
        with_cursor(src, |c| {
            let regions: Vec<_> = c.folding.iter()
                .filter(|f| f.kind == Some(FoldingRangeKind::Region))
                .collect();
            assert_eq!(regions.len(), 2);
        });
    }

    #[test]
    fn folding_comments() {
        let src = "// a\n// b\n// c\ntype handle extends agent\n";
        with_cursor(src, |c| {
            let cmt: Vec<_> = c.folding.iter()
                .filter(|f| f.kind == Some(FoldingRangeKind::Comment))
                .collect();
            assert_eq!(cmt.len(), 1);
            assert_eq!(cmt[0].start_line, 0);
            assert_eq!(cmt[0].end_line, 2);
        });
    }

    #[test]
    fn scope_params_initialized() {
        let src = "\
function Foo takes integer x, real y returns nothing
endfunction
";
        with_cursor(src, |c| {
            let s = c.scopes.iter().find(|s| s.name == "Foo").unwrap();
            assert!(s.vars["x"].is_initialized);
            assert!(s.vars["y"].is_initialized);
        });
    }

    #[test]
    fn scope_local_set() {
        let src = "\
function Foo takes nothing returns nothing
    local integer x
    set x = 5
endfunction
";
        with_cursor(src, |c| {
            let s = c.scopes.iter().find(|s| s.name == "Foo").unwrap();
            assert!(s.vars["x"].is_initialized);
        });
    }

    #[test]
    fn scope_local_uninitialized() {
        let src = "\
function Foo takes nothing returns nothing
    local integer x
endfunction
";
        with_cursor(src, |c| {
            let s = c.scopes.iter().find(|s| s.name == "Foo").unwrap();
            assert!(!s.vars["x"].is_initialized);
        });
    }

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
            assert!(tok.is_some(), "Should have token for 'MyFunc', tokens: {:?}", line.tokens);
            assert_eq!(tok.unwrap().kind, TokenKind::Function);
        });
    }

    #[test]
    fn semantic_type_name_is_type() {
        let src = "type handle extends agent\n";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            let tok = line.tokens.iter().find(|t| t.col == 5 && t.len == 6);
            assert!(tok.is_some(), "Should have token for 'handle', tokens: {:?}", line.tokens);
            assert_eq!(tok.unwrap().kind, TokenKind::Type);
            let tok2 = line.tokens.iter().find(|t| t.col == 20 && t.len == 5);
            assert!(tok2.is_some(), "Should have token for 'agent', tokens: {:?}", line.tokens);
            assert_eq!(tok2.unwrap().kind, TokenKind::Type);
        });
    }

    #[test]
    fn diagnostics_from_errors() {
        let src = "function\n";
        with_cursor(src, |c| {
            assert!(!c.diagnostics.is_empty());
        });
    }

    #[test]
    fn full_program() {
        let src = "\
type handle extends agent
native Ack takes integer m, integer n returns integer
globals
    integer g
endglobals
function main takes nothing returns nothing
    local integer x = 1
    set x = 2
    call Ack(x, x)
    if true then
        return
    endif
endfunction
";
        with_cursor(src, |c| {
            assert_eq!(c.symbols.len(), 4);
            assert_eq!(c.scopes.len(), 2);
            assert!(!c.semantic.data(None).is_empty());
            assert!(c.diagnostics.is_empty());
        });
    }

    #[test]
    fn semantic_string_literal() {
        let src = "call Foo(\"my shit\")\n";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            let str_tok = line.tokens.iter().find(|t| t.col == 9);
            assert!(
                str_tok.is_some(),
                "Should have a string token at col=9, tokens: {:?}",
                line.tokens
            );
            assert_eq!(
                str_tok.unwrap().kind,
                TokenKind::String,
                "String literal should be TokenKind::String, got {:?}",
                str_tok.unwrap().kind
            );
            assert_eq!(str_tok.unwrap().len, 9, "String token len should be 9 (including quotes)");
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
            assert!(ul_tok.is_some(), "Should have UnitLife token, tokens: {:?}", line.tokens);
            assert_eq!(ul_tok.unwrap().kind, TokenKind::Function,
                "UnitLife should be Function, got {:?}", ul_tok.unwrap().kind);
            let ih_tok = line.tokens.iter().find(|t| t.col == 40 && t.len == 8);
            assert!(ih_tok.is_some(), "Should have IsHidden token, tokens: {:?}", line.tokens);
            assert_eq!(ih_tok.unwrap().kind, TokenKind::Function,
                "IsHidden should be Function, got {:?}", ih_tok.unwrap().kind);
        });
    }
}

