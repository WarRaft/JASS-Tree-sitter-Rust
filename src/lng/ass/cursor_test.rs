#[cfg(test)]
mod tests {
    use crate::lng::ass::ast::*;
    use crate::lng::ass::cursor::Cursor;
    use crate::http::semantic::token::Kind as TokenKind;
    use lapce_xi_rope::Rope;

    fn with_cursor(src: &str, f: impl FnOnce(&Cursor)) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_as::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let mut ast = build_ast(tree.root_node());
        let rope = Rope::from(src);
        let src_bytes: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
        rewrite_directives(&mut ast, &src_bytes);
        let cursor = Cursor::walk(&ast, &rope, &[]);
        f(&cursor);
    }

    fn with_cursor_imported(
        src: &str,
        imported: &[crate::lng::ass::cursor::ImportedSymbol],
        f: impl FnOnce(&Cursor),
    ) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_as::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let mut ast = build_ast(tree.root_node());
        let rope = Rope::from(src);
        let src_bytes: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
        rewrite_directives(&mut ast, &src_bytes);
        let cursor = Cursor::walk(&ast, &rope, imported);
        f(&cursor);
    }

    fn collect_tokens(src: &str, cursor: &Cursor) -> Vec<(String, TokenKind)> {
        let mut result = Vec::new();
        for (_line_idx, line) in &cursor.semantic.lines {
            for token in &line.tokens {
                // token.col / token.len are in UTF-16 code units;
                // iterate chars while accumulating UTF-16 widths.
                let text: String = src.lines()
                    .nth(token.row)
                    .map(|l| {
                        let mut utf16_pos = 0;
                        let mut out = String::new();
                        for ch in l.chars() {
                            let w = ch.len_utf16();
                            if utf16_pos >= token.col + token.len { break; }
                            if utf16_pos >= token.col {
                                out.push(ch);
                            }
                            utf16_pos += w;
                        }
                        out
                    })
                    .unwrap_or_default();
                result.push((text, token.kind));
            }
        }
        result
    }

    #[test]
    fn import_directive_colored() {
        let src = "//import ../common.j\nvoid main() {}\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let import_token = tokens.iter().find(|(t, _)| t == "//import");
            assert!(import_token.is_some(), "Expected //import token, got: {:?}", tokens);
            assert_eq!(import_token.unwrap().1, TokenKind::Macro);

            let path_token = tokens.iter().find(|(t, _)| t == "../common.j");
            assert!(path_token.is_some(), "Expected path token, got: {:?}", tokens);
            assert_eq!(path_token.unwrap().1, TokenKind::String);
        });
    }

    #[test]
    fn import_only_file_colored() {
        // Edge case: file with ONLY //import directives, no code
        let src = "//import ../common.j\n//import! ../blizzard.j\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let import_tokens: Vec<_> = tokens.iter()
                .filter(|(_, k)| *k == TokenKind::Macro)
                .collect();
            assert_eq!(import_tokens.len(), 2,
                "Expected 2 Macro tokens (//import, //import!), got: {:?}", tokens);

            let string_tokens: Vec<_> = tokens.iter()
                .filter(|(_, k)| *k == TokenKind::String)
                .collect();
            assert_eq!(string_tokens.len(), 2,
                "Expected 2 String tokens (paths), got: {:?}", tokens);
        });
    }

    #[test]
    fn import_set_ignore_all_colored() {
        let src = "//import ../common.j\n//set hint ref\n//ignore unused\nvoid main() {}\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            // //import → Macro
            assert!(tokens.iter().any(|(t, k)| t == "//import" && *k == TokenKind::Macro),
                "Missing //import Macro token: {:?}", tokens);
            // //set → Macro
            assert!(tokens.iter().any(|(t, k)| t == "//set" && *k == TokenKind::Macro),
                "Missing //set Macro token: {:?}", tokens);
            // //ignore → Macro
            assert!(tokens.iter().any(|(t, k)| t == "//ignore" && *k == TokenKind::Macro),
                "Missing //ignore Macro token: {:?}", tokens);

            // Semantic data encoding round-trip
            let data = cursor.semantic.data(None);
            assert!(!data.is_empty(), "semantic data should not be empty");
        });
    }

    #[test]
    fn import_ujapi_directive_colored() {
        let src = "//import-ujapi! ../ujapi/common.j\nvoid main() {}\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let prefix_token = tokens.iter().find(|(t, _)| t == "//import-ujapi!");
            assert!(prefix_token.is_some(), "Expected //import-ujapi! token, got: {:?}", tokens);
            assert_eq!(prefix_token.unwrap().1, TokenKind::Macro);

            let path_token = tokens.iter().find(|(t, _)| t == "../ujapi/common.j");
            assert!(path_token.is_some(), "Expected path token, got: {:?}", tokens);
            assert_eq!(path_token.unwrap().1, TokenKind::String);
        });
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

    // ─── Class / method semantic tokens ──────────────────────────────────

    #[test]
    fn class_method_tokens() {
        let src = "\
class Foo {
    int bar(int x) {
        return x;
    }
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let foo = tokens.iter().find(|(t, _)| t == "Foo").unwrap();
            assert_eq!(foo.1, TokenKind::Type,
                "Expected 'Foo' to be Type (ClassDecl), got {:?}", foo.1);

            let bar = tokens.iter().find(|(t, _)| t == "bar").unwrap();
            assert_eq!(bar.1, TokenKind::Function,
                "Expected 'bar' to be Function, got {:?}", bar.1);

            let x_params: Vec<_> = tokens.iter().filter(|(t, _)| t == "x").collect();
            assert!(x_params.iter().any(|(_, k)| *k == TokenKind::Parameter),
                "Expected at least one 'x' as Parameter, got {:?}", x_params);
        });
    }

    // ─── Enum semantic tokens ────────────────────────────────────────────

    #[test]
    fn enum_tokens() {
        let src = "\
enum Color {
    Red,
    Green = 1,
    Blue
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let color = tokens.iter().find(|(t, _)| t == "Color").unwrap();
            assert_eq!(color.1, TokenKind::Enum,
                "Expected 'Color' to be Enum, got {:?}", color.1);

            for member in &["Red", "Green", "Blue"] {
                let tok = tokens.iter().find(|(t, _)| t == member).unwrap();
                assert_eq!(tok.1, TokenKind::EnumMember,
                    "Expected '{}' to be EnumMember, got {:?}", member, tok.1);
            }
        });
    }

    // ─── Namespace semantic tokens ───────────────────────────────────────

    #[test]
    fn namespace_tokens() {
        let src = "\
namespace MyNs {
    void helper() {}
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let ns = tokens.iter().find(|(t, _)| t == "MyNs").unwrap();
            assert_eq!(ns.1, TokenKind::Namespace,
                "Expected 'MyNs' to be Namespace, got {:?}", ns.1);

            let helper = tokens.iter().find(|(t, _)| t == "helper").unwrap();
            assert_eq!(helper.1, TokenKind::Function,
                "Expected 'helper' to be Function, got {:?}", helper.1);
        });
    }

    // ─── Document symbols ────────────────────────────────────────────────

    use crate::http::document_symbol::SymbolKind;

    #[test]
    fn function_symbol() {
        let src = "void main() {}\n";
        with_cursor(src, |cursor| {
            assert_eq!(cursor.symbols.len(), 1);
            assert_eq!(cursor.symbols[0].name, "main");
            assert_eq!(cursor.symbols[0].kind, SymbolKind::Function);
        });
    }

    #[test]
    fn class_symbol_with_methods() {
        let src = "\
class Foo {
    int bar() { return 1; }
    void baz() {}
}
";
        with_cursor(src, |cursor| {
            assert_eq!(cursor.symbols.len(), 1);
            let cls = &cursor.symbols[0];
            assert_eq!(cls.name, "Foo");
            assert_eq!(cls.kind, SymbolKind::Class);

            let children = cls.children.as_ref().unwrap();
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].name, "bar");
            assert_eq!(children[0].kind, SymbolKind::Function);
            assert_eq!(children[1].name, "baz");
        });
    }

    #[test]
    fn enum_symbol_with_members() {
        let src = "\
enum Dir {
    Up,
    Down
}
";
        with_cursor(src, |cursor| {
            assert_eq!(cursor.symbols.len(), 1);
            let en = &cursor.symbols[0];
            assert_eq!(en.name, "Dir");
            assert_eq!(en.kind, SymbolKind::Enum);

            let children = en.children.as_ref().unwrap();
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].name, "Up");
            assert_eq!(children[0].kind, SymbolKind::EnumMember);
            assert_eq!(children[1].name, "Down");
        });
    }

    #[test]
    fn namespace_symbol_nested() {
        let src = "\
namespace Outer {
    void inner_fn() {}
}
";
        with_cursor(src, |cursor| {
            assert_eq!(cursor.symbols.len(), 1);
            let ns = &cursor.symbols[0];
            assert_eq!(ns.name, "Outer");
            assert_eq!(ns.kind, SymbolKind::Namespace);

            let children = ns.children.as_ref().unwrap();
            assert_eq!(children.len(), 1);
            assert_eq!(children[0].name, "inner_fn");
            assert_eq!(children[0].kind, SymbolKind::Function);
        });
    }

    // ─── Folding ranges ─────────────────────────────────────────────────

    use crate::http::folding::FoldingRangeKind;

    #[test]
    fn function_folding() {
        let src = "\
void main() {
    int x = 1;
    int y = 2;
}
";
        with_cursor(src, |cursor| {
            let regions: Vec<_> = cursor.folding.iter()
                .filter(|f| f.kind == Some(FoldingRangeKind::Region))
                .collect();
            assert!(!regions.is_empty(), "Expected at least one Region folding range");
            assert_eq!(regions[0].start_line, 0);
            assert_eq!(regions[0].end_line, 3);
        });
    }

    #[test]
    fn comment_run_folding() {
        let src = "\
// line 1
// line 2
// line 3
void main() {}
";
        with_cursor(src, |cursor| {
            let comments: Vec<_> = cursor.folding.iter()
                .filter(|f| f.kind == Some(FoldingRangeKind::Comment))
                .collect();
            assert!(!comments.is_empty(),
                "Expected Comment folding range for consecutive comments, got: {:?}",
                cursor.folding);
            assert_eq!(comments[0].start_line, 0);
            assert_eq!(comments[0].end_line, 2);
        });
    }

    // ─── Literals & keywords ─────────────────────────────────────────────

    #[test]
    fn literal_tokens() {
        let src = "\
void main() {
    int a = 42;
    float b = 3.14;
    bool c = true;
    string d = \"hello\";
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let num42 = tokens.iter().find(|(t, _)| t == "42").unwrap();
            assert_eq!(num42.1, TokenKind::Number,
                "Expected '42' to be Number, got {:?}", num42.1);

            let num_pi = tokens.iter().find(|(t, _)| t == "3.14").unwrap();
            assert_eq!(num_pi.1, TokenKind::Number,
                "Expected '3.14' to be Number, got {:?}", num_pi.1);

            let kw_true = tokens.iter().find(|(t, _)| t == "true").unwrap();
            assert_eq!(kw_true.1, TokenKind::Number,
                "Expected 'true' to be Number (literal), got {:?}", kw_true.1);

            let str_hello = tokens.iter().find(|(t, _)| t.contains("hello")).unwrap();
            assert_eq!(str_hello.1, TokenKind::String,
                "Expected string to be String, got {:?}", str_hello.1);

            // Keywords
            let kw_void = tokens.iter().find(|(t, _)| t == "void").unwrap();
            assert_eq!(kw_void.1, TokenKind::Type,
                "Expected 'void' to be Type (primitive keyword), got {:?}", kw_void.1);

            let kw_int = tokens.iter().find(|(t, _)| t == "int").unwrap();
            assert_eq!(kw_int.1, TokenKind::Type,
                "Expected 'int' to be Type, got {:?}", kw_int.1);

            let kw_float = tokens.iter().find(|(t, _)| t == "float").unwrap();
            assert_eq!(kw_float.1, TokenKind::Type,
                "Expected 'float' to be Type, got {:?}", kw_float.1);

            let kw_bool = tokens.iter().find(|(t, _)| t == "bool").unwrap();
            assert_eq!(kw_bool.1, TokenKind::Type,
                "Expected 'bool' to be Type, got {:?}", kw_bool.1);

            let kw_string = tokens.iter().find(|(t, _)| t == "string").unwrap();
            assert_eq!(kw_string.1, TokenKind::Type,
                "Expected 'string' to be Type, got {:?}", kw_string.1);
        });
    }

    #[test]
    fn return_keyword_token() {
        let src = "\
int get() {
    return 0;
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let kw_return = tokens.iter().find(|(t, _)| t == "return").unwrap();
            assert_eq!(kw_return.1, TokenKind::Keyword,
                "Expected 'return' to be Keyword, got {:?}", kw_return.1);
        });
    }

    // ─── Doc comment (//*) ───────────────────────────────────────────────

    #[test]
    fn doc_comment_colored() {
        let src = "\
//* This is a doc comment
void main() {}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let prefix = tokens.iter().find(|(t, _)| t == "//*").unwrap();
            assert_eq!(prefix.1, TokenKind::Comment,
                "Expected '//*' prefix to be Comment, got {:?}", prefix.1);

            let body = tokens.iter().find(|(t, _)| t.contains("This is a doc comment"));
            assert!(body.is_some(),
                "Expected doc-comment body token, got: {:?}", tokens);
            assert_eq!(body.unwrap().1, TokenKind::String,
                "Expected doc body to be String, got {:?}", body.unwrap().1);
        });
    }

    // ─── @ignore comment ─────────────────────────────────────────────────

    #[test]
    fn at_ignore_comment_colored() {
        let src = "\
void main() {
    //@ignore unused deprecated
    int x = 0;
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let ignore = tokens.iter().find(|(t, _)| t == "//@ignore").unwrap();
            assert_eq!(ignore.1, TokenKind::Macro,
                "Expected '//@ignore' to be Macro, got {:?}", ignore.1);

            assert!(tokens.iter().any(|(t, k)| t == "unused" && *k == TokenKind::Property),
                "Expected 'unused' as Property, got: {:?}", tokens);
            assert!(tokens.iter().any(|(t, k)| t == "deprecated" && *k == TokenKind::Property),
                "Expected 'deprecated' as Property, got: {:?}", tokens);
        });
    }

    // ─── Member access & call expression ─────────────────────────────────

    #[test]
    fn member_access_tokens() {
        let src = "\
void main() {
    int x = obj.value;
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let member = tokens.iter().find(|(t, _)| t == "value").unwrap();
            assert_eq!(member.1, TokenKind::Property,
                "Expected 'value' to be Property (member access), got {:?}", member.1);
        });
    }

    // ─── Typedef & funcdef ───────────────────────────────────────────────

    #[test]
    fn typedef_tokens() {
        let src = "typedef int MyInt;\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            assert!(tokens.iter().any(|(t, k)| t == "MyInt" && *k == TokenKind::Type),
                "Expected 'MyInt' as Type (typedef alias), got: {:?}", tokens);

            // symbol
            assert_eq!(cursor.symbols.len(), 1);
            assert_eq!(cursor.symbols[0].name, "MyInt");
            assert_eq!(cursor.symbols[0].kind, SymbolKind::TypeParameter);
        });
    }

    #[test]
    fn funcdef_tokens() {
        let src = "funcdef void Callback(int x);\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let cb = tokens.iter().find(|(t, _)| t == "Callback").unwrap();
            assert_eq!(cb.1, TokenKind::Function,
                "Expected 'Callback' to be Function (funcdef name), got {:?}", cb.1);

            assert_eq!(cursor.symbols.len(), 1);
            assert_eq!(cursor.symbols[0].name, "Callback");
            assert_eq!(cursor.symbols[0].kind, SymbolKind::Function);
        });
    }

    // ─── Interface ───────────────────────────────────────────────────────

    #[test]
    fn interface_tokens_and_symbol() {
        let src = "\
interface IAnimal {
    void speak();
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let iface = tokens.iter().find(|(t, _)| t == "IAnimal").unwrap();
            assert_eq!(iface.1, TokenKind::Type,
                "Expected 'IAnimal' to be Type (InterfaceDecl), got {:?}", iface.1);

            assert_eq!(cursor.symbols.len(), 1);
            let sym = &cursor.symbols[0];
            assert_eq!(sym.name, "IAnimal");
            assert_eq!(sym.kind, SymbolKind::Interface);

            let children = sym.children.as_ref().unwrap();
            assert_eq!(children.len(), 1);
            assert_eq!(children[0].name, "speak");
        });
    }

    // ─── File settings via //set ─────────────────────────────────────────

    #[test]
    fn set_directive_populates_settings() {
        let src = "//set hint ref\nvoid main() {}\n";
        with_cursor(src, |cursor| {
            assert_eq!(cursor.file_settings.get("hint").map(|s| s.as_str()),
                Some("ref"),
                "Expected file_settings[\"hint\"] = \"ref\", got: {:?}", cursor.file_settings);
        });
    }

    // ─── File ignore tags via //ignore ───────────────────────────────────

    #[test]
    fn ignore_directive_populates_tags() {
        let src = "//ignore unused\nvoid main() {}\n";
        with_cursor(src, |cursor| {
            assert!(cursor.file_ignore_tags.contains("unused"),
                "Expected 'unused' in file_ignore_tags, got: {:?}", cursor.file_ignore_tags);
        });
    }

    // ─── Global variable ─────────────────────────────────────────────────

    #[test]
    fn global_var_symbol() {
        let src = "int globalCounter = 0;\n";
        with_cursor(src, |cursor| {
            assert_eq!(cursor.symbols.len(), 1);
            assert_eq!(cursor.symbols[0].name, "globalCounter");
            assert_eq!(cursor.symbols[0].kind, SymbolKind::Variable);
            assert_eq!(cursor.symbols[0].detail.as_deref(), Some("int"));
        });
    }

    // ─── Diagnostics for syntax errors ───────────────────────────────────

    #[test]
    fn syntax_error_produces_diagnostic() {
        let src = "void main( {}\n";
        with_cursor(src, |cursor| {
            assert!(!cursor.diagnostics.is_empty(),
                "Expected at least one diagnostic for syntax error");
        });
    }

    // ─── Empty source ────────────────────────────────────────────────────

    #[test]
    fn empty_source() {
        let src = "";
        with_cursor(src, |cursor| {
            assert!(cursor.symbols.is_empty());
            assert!(cursor.diagnostics.is_empty());
            assert!(cursor.folding.is_empty());
        });
    }

    // ─── Multiple functions ──────────────────────────────────────────────

    #[test]
    fn multiple_functions_symbols() {
        let src = "\
void foo() {}
void bar() {}
void baz() {}
";
        with_cursor(src, |cursor| {
            assert_eq!(cursor.symbols.len(), 3);
            assert_eq!(cursor.symbols[0].name, "foo");
            assert_eq!(cursor.symbols[1].name, "bar");
            assert_eq!(cursor.symbols[2].name, "baz");
        });
    }

    // ─── Semantic data encoding ──────────────────────────────────────────

    #[test]
    fn semantic_data_not_empty_for_code() {
        let src = "void main() { int x = 1; }\n";
        with_cursor(src, |cursor| {
            let data = cursor.semantic.data(None);
            assert!(!data.is_empty(),
                "semantic data should contain entries for tokens");
        });
    }

    #[test]
    fn semantic_data_empty_for_empty_source() {
        let src = "";
        with_cursor(src, |cursor| {
            let data = cursor.semantic.data(None);
            assert!(data.is_empty(),
                "semantic data should be empty for empty source");
        });
    }

    // ─── Composite / generic types ───────────────────────────────────────

    #[test]
    fn array_type_inner_type_colored() {
        let src = "array<unit> PlayerUnit;\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let array_tok = tokens.iter().find(|(t, _)| t == "array").unwrap();
            assert_eq!(array_tok.1, TokenKind::Type,
                "Expected 'array' to be Type, got {:?}", array_tok.1);

            let unit_tok = tokens.iter().find(|(t, _)| t == "unit").unwrap();
            assert_eq!(unit_tok.1, TokenKind::Type,
                "Expected 'unit' (inner type) to be Type, got {:?}", unit_tok.1);

            // detail should be the full composite type
            assert_eq!(cursor.symbols.len(), 1);
            assert_eq!(cursor.symbols[0].detail.as_deref(), Some("array<unit>"));
        });
    }

    #[test]
    fn array_type_in_param_colored() {
        let src = "void foo(array<int> param) {}\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let array_tok = tokens.iter().find(|(t, _)| t == "array").unwrap();
            assert_eq!(array_tok.1, TokenKind::Type,
                "Expected 'array' to be Type, got {:?}", array_tok.1);

            // 'int' as inner type in array<int> should still be Type
            let int_toks: Vec<_> = tokens.iter().filter(|(t, _)| t == "int").collect();
            assert!(int_toks.iter().all(|(_, k)| *k == TokenKind::Type),
                "Expected all 'int' tokens to be Type, got {:?}", int_toks);
        });
    }

    #[test]
    fn array_weathereffect_type_colored() {
        let src = "array<weathereffect> effects;\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let array_tok = tokens.iter().find(|(t, _)| t == "array").unwrap();
            assert_eq!(array_tok.1, TokenKind::Type,
                "Expected 'array' to be Type, got {:?}", array_tok.1);

            let we_tok = tokens.iter().find(|(t, _)| t == "weathereffect").unwrap();
            assert_eq!(we_tok.1, TokenKind::Type,
                "Expected 'weathereffect' (inner type) to be Type, got {:?}", we_tok.1);

            assert_eq!(cursor.symbols[0].detail.as_deref(), Some("array<weathereffect>"));
        });
    }

    #[test]
    fn array_builtin_no_undeclared_diagnostic() {
        // `array` is a built-in template type — should not produce "Undeclared type"
        let src = "array<int> numbers;\n";
        with_cursor(src, |cursor| {
            let undeclared: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("array"))
                .collect();
            assert!(undeclared.is_empty(),
                "Expected no 'undeclared' diagnostic for built-in 'array', got {:?}", undeclared);
        });
    }

    #[test]
    fn array_inner_custom_type_unresolved_produces_diagnostic() {
        // Inner type `Foo` is not declared → should still produce diagnostic
        let src = "array<Foo> items;\n";
        with_cursor(src, |cursor| {
            let foo_diag: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("Foo"))
                .collect();
            assert!(!foo_diag.is_empty(),
                "Expected 'undeclared' diagnostic for unknown inner type 'Foo'");

            // But no diagnostic for 'array' itself
            let array_diag: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("array"))
                .collect();
            assert!(array_diag.is_empty(),
                "Expected no diagnostic for built-in 'array', got {:?}", array_diag);
        });
    }

    // ─── Built-in funcdef types ──────────────────────────────────────────

    #[test]
    fn builtin_callback_func_no_diagnostic() {
        let src = "CallbackFunc cb;\n";
        with_cursor(src, |cursor| {
            let diag: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("CallbackFunc"))
                .collect();
            assert!(diag.is_empty(),
                "Expected no diagnostic for built-in 'CallbackFunc', got {:?}", diag);
        });
    }

    #[test]
    fn builtin_boolexpr_func_no_diagnostic() {
        let src = "BoolexprFunc filter;\n";
        with_cursor(src, |cursor| {
            let diag: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("BoolexprFunc"))
                .collect();
            assert!(diag.is_empty(),
                "Expected no diagnostic for built-in 'BoolexprFunc', got {:?}", diag);
        });
    }

    #[test]
    fn builtin_funcdef_types_colored_as_type() {
        let src = "CallbackFunc cb;\nBoolexprFunc filter;\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let cb_tok = tokens.iter().find(|(t, _)| t == "CallbackFunc").unwrap();
            assert_eq!(cb_tok.1, TokenKind::Type,
                "Expected 'CallbackFunc' to be Type, got {:?}", cb_tok.1);

            let be_tok = tokens.iter().find(|(t, _)| t == "BoolexprFunc").unwrap();
            assert_eq!(be_tok.1, TokenKind::Type,
                "Expected 'BoolexprFunc' to be Type, got {:?}", be_tok.1);
        });
    }

    // ─── Function references (@FuncName) ─────────────────────────────────

    #[test]
    fn handle_of_func_ref_colored_as_function() {
        let src = "void test() {\n  ForGroup(g, @MyFunc);\n}\nvoid MyFunc() {}\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let my_func_tok = tokens.iter().find(|(t, _)| t == "MyFunc"
                && *t != "ForGroup").unwrap();
            assert_eq!(my_func_tok.1, TokenKind::Function,
                "Expected '@MyFunc' operand to be Function, got {:?}", my_func_tok.1);
        });
    }

    #[test]
    fn handle_of_func_ref_linked_to_decl() {
        let src = "void MyFunc() {}\nvoid test() {\n  ForGroup(g, @MyFunc);\n}\n";
        with_cursor(src, |cursor| {
            // MyFunc should appear in ref_groups — both decl and ref
            let my_func_key = cursor.ref_names.iter()
                .find(|(_, n)| n.as_str() == "MyFunc")
                .map(|(k, _)| *k);
            assert!(my_func_key.is_some(),
                "Expected MyFunc in ref_names");

            let key = my_func_key.unwrap();
            let occurrences = cursor.ref_groups.get(&key).unwrap();
            // At least 2: declaration + @MyFunc reference
            assert!(occurrences.len() >= 2,
                "Expected at least 2 occurrences for MyFunc (decl + @ref), got {}",
                occurrences.len());
        });
    }

    // ─── Char literal ────────────────────────────────────────────────────

    #[test]
    fn char_literal_colored_as_number() {
        let src = "int x = 'A';\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let char_tok = tokens.iter().find(|(t, _)| t == "'A'").unwrap();
            assert_eq!(char_tok.1, TokenKind::Number,
                "Expected char literal 'A' to be Number, got {:?}", char_tok.1);
        });
    }

    #[test]
    fn string_literal_still_colored_as_string() {
        let src = "string s = \"hello\";\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            // String literal is tokenized into sub-ranges (quotes + content).
            // All parts (", hello, ") should be String.
            let str_parts: Vec<_> = tokens.iter()
                .filter(|(t, _)| t == "\"" || t == "hello")
                .collect();
            assert!(!str_parts.is_empty(),
                "Expected string literal tokens, got: {:?}", tokens);
            for (text, kind) in &str_parts {
                assert_eq!(*kind, TokenKind::String,
                    "Expected '{}' to be String, got {:?}", text, kind);
            }
        });
    }

    // ─── Namespace-qualified function calls ───────────────────────────────

    #[test]
    fn namespace_qualified_call_no_undeclared_with_import() {
        use crate::lng::ass::cursor::{ImportedKind, ImportedSymbol};

        let src = "\
namespace Jass {
    void UnitItemInSlot() {}
}
void main() {
    Jass::UnitItemInSlot();
}
";
        let imported = vec![
            ImportedSymbol {
                origin_uri: url::Url::parse("file:///common.j").unwrap(),
                name: "UnitItemInSlot".to_string(),
                kind: ImportedKind::Func,
                origin_decl_key: None,
                return_type: Some("item".to_string()),
                type_name: None,
                namespace: "Jass".to_string(),
            },
        ];
        with_cursor_imported(src, &imported, |cursor| {
            let undeclared: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("UnitItemInSlot"))
                .collect();
            assert!(undeclared.is_empty(),
                "Expected no 'undeclared' diagnostic for Jass::UnitItemInSlot, got {:?}", undeclared);
        });
    }

    #[test]
    fn namespace_qualified_call_tokens() {
        let src = "\
namespace Jass {
    void UnitItemInSlot() {}
}
void main() {
    Jass::UnitItemInSlot();
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            // The namespace part should be Namespace
            let jass_tokens: Vec<_> = tokens.iter()
                .filter(|(t, _)| t == "Jass")
                .collect();
            assert!(jass_tokens.iter().any(|(_, k)| *k == TokenKind::Namespace),
                "Expected 'Jass' as Namespace in call, got: {:?}", jass_tokens);

            // The function name part should be Function
            let func_tokens: Vec<_> = tokens.iter()
                .filter(|(t, _)| t == "UnitItemInSlot")
                .collect();
            assert!(func_tokens.iter().any(|(_, k)| *k == TokenKind::Function),
                "Expected 'UnitItemInSlot' as Function in call, got: {:?}", func_tokens);
        });
    }

    #[test]
    fn namespace_qualified_call_undeclared_without_import() {
        // Without Jass namespace declared and without imports,
        // Jass::UnknownFunc should produce an undeclared diagnostic.
        let src = "\
void main() {
    Jass::UnknownFunc();
}
";
        with_cursor(src, |cursor| {
            let undeclared: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("UnknownFunc"))
                .collect();
            assert!(!undeclared.is_empty(),
                "Expected 'undeclared' diagnostic for Jass::UnknownFunc without imports");
        });
    }

    // ─── Handle assignment (@var = expr) ──────────────────────────────────

    #[test]
    fn handle_assign_var_no_undeclared() {
        // @dummyDamageCallback = cb — handle assignment to a variable
        // should NOT produce "undeclared" diagnostic for dummyDamageCallback
        let src = "\
funcdef void DamageCallbackFn(int x);
class UnitData {
    DamageCallbackFn@ dummyDamageCallback;
    void SetDamageCallback(DamageCallbackFn@ cb) {
        @dummyDamageCallback = cb;
    }
}
";
        with_cursor(src, |cursor| {
            let undeclared: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("dummyDamageCallback"))
                .collect();
            assert!(undeclared.is_empty(),
                "Expected no 'undeclared' diagnostic for @dummyDamageCallback in handle assignment, got {:?}", undeclared);
        });
    }

    #[test]
    fn handle_assign_var_colored_as_variable() {
        let src = "\
funcdef void DamageCallbackFn(int x);
class UnitData {
    DamageCallbackFn@ dummyDamageCallback;
    void SetDamageCallback(DamageCallbackFn@ cb) {
        @dummyDamageCallback = cb;
    }
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            // dummyDamageCallback in @dummyDamageCallback should be Variable, not Function
            let dd_tokens: Vec<_> = tokens.iter()
                .filter(|(t, _)| t == "dummyDamageCallback")
                .collect();
            assert!(dd_tokens.iter().all(|(_, k)| *k == TokenKind::Variable),
                "Expected all 'dummyDamageCallback' tokens to be Variable, got {:?}", dd_tokens);
        });
    }

    #[test]
    fn funcdef_handle_type_in_method_param_no_undeclared() {
        // DamageCallbackFn@ as a method parameter type should resolve
        let src = "\
funcdef void DamageCallbackFn(int x);
class UnitData {
    void SetDamageCallback(DamageCallbackFn@ cb) {}
}
";
        with_cursor(src, |cursor| {
            let undeclared: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("DamageCallbackFn"))
                .collect();
            assert!(undeclared.is_empty(),
                "Expected no 'undeclared' diagnostic for DamageCallbackFn@ in method param, got {:?}", undeclared);
        });
    }

    #[test]
    fn funcdef_handle_type_in_method_param_colored_as_type() {
        let src = "\
funcdef void DamageCallbackFn(int x);
class UnitData {
    void SetDamageCallback(DamageCallbackFn@ cb) {}
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            let dcf_tokens: Vec<_> = tokens.iter()
                .filter(|(t, _)| t == "DamageCallbackFn")
                .collect();
            // All DamageCallbackFn tokens should be Type (funcdef is a type declaration)
            // except the funcdef name itself which is Function
            assert!(dcf_tokens.len() >= 2,
                "Expected at least 2 DamageCallbackFn tokens (decl + usage), got {:?}", dcf_tokens);
        });
    }

    #[test]
    fn funcdef_handle_global_var_no_undeclared() {
        // Global variable with funcdef handle type should work
        let src = "\
funcdef void DamageCallbackFn(int x);
DamageCallbackFn@ gDmg_Callback = null;
";
        with_cursor(src, |cursor| {
            let undeclared: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("DamageCallbackFn"))
                .collect();
            assert!(undeclared.is_empty(),
                "Expected no 'undeclared' diagnostic for DamageCallbackFn@ at global scope, got {:?}", undeclared);
        });
    }

    // ─── Class member resolution (this. and bare) ──────────────────────────

    #[test]
    fn class_bare_property_resolves() {
        // Bare `a` inside a method should resolve to the class property `a`
        let src = "\
class A {
    int a = 3;
    void b(int d) {
        a = 4;
    }
}
";
        with_cursor(src, |cursor| {
            let a_key = cursor.ref_names.iter()
                .find(|(_, n)| n.as_str() == "a")
                .map(|(k, _)| *k);
            assert!(a_key.is_some(), "Expected 'a' in ref_names");
            let occurrences = cursor.ref_groups.get(&a_key.unwrap()).unwrap();
            // At least 2: declaration + bare reference inside method b
            assert!(occurrences.len() >= 2,
                "Expected at least 2 occurrences for 'a' (decl + bare ref), got {}",
                occurrences.len());
        });
    }

    #[test]
    fn class_this_property_resolves() {
        // `this.a` inside a method should resolve to the class property `a`
        let src = "\
class A {
    int a = 3;
    void b() {
        this.a = 5;
    }
}
";
        with_cursor(src, |cursor| {
            let a_key = cursor.ref_names.iter()
                .find(|(_, n)| n.as_str() == "a")
                .map(|(k, _)| *k);
            assert!(a_key.is_some(), "Expected 'a' in ref_names");
            let occurrences = cursor.ref_groups.get(&a_key.unwrap()).unwrap();
            // At least 2: declaration + this.a reference
            assert!(occurrences.len() >= 2,
                "Expected at least 2 occurrences for 'a' (decl + this.a ref), got {}",
                occurrences.len());
        });
    }

    #[test]
    fn class_bare_method_call_resolves() {
        // Bare `b(a)` inside method c should resolve to method b and property a
        let src = "\
class A {
    int a = 3;
    void b(int d) {}
    void c() {
        b(a);
    }
}
";
        with_cursor(src, |cursor| {
            let b_key = cursor.ref_names.iter()
                .find(|(_, n)| n.as_str() == "b")
                .map(|(k, _)| *k);
            assert!(b_key.is_some(), "Expected 'b' in ref_names");
            let b_occs = cursor.ref_groups.get(&b_key.unwrap()).unwrap();
            assert!(b_occs.len() >= 2,
                "Expected at least 2 occurrences for 'b' (decl + call ref), got {}",
                b_occs.len());

            let a_key = cursor.ref_names.iter()
                .find(|(_, n)| n.as_str() == "a")
                .map(|(k, _)| *k);
            assert!(a_key.is_some(), "Expected 'a' in ref_names");
            let a_occs = cursor.ref_groups.get(&a_key.unwrap()).unwrap();
            assert!(a_occs.len() >= 2,
                "Expected at least 2 occurrences for 'a' (decl + arg ref), got {}",
                a_occs.len());
        });
    }

    #[test]
    fn class_this_method_call_resolves() {
        // `this.b(this.a)` should resolve both method b and property a
        let src = "\
class A {
    int a = 3;
    void b(int d) {}
    void c() {
        this.b(this.a);
    }
}
";
        with_cursor(src, |cursor| {
            let b_key = cursor.ref_names.iter()
                .find(|(_, n)| n.as_str() == "b")
                .map(|(k, _)| *k);
            assert!(b_key.is_some(), "Expected 'b' in ref_names");
            let b_occs = cursor.ref_groups.get(&b_key.unwrap()).unwrap();
            // At least 2: declaration + this.b(...) call
            assert!(b_occs.len() >= 2,
                "Expected at least 2 occurrences for 'b' (decl + this.b() ref), got {}",
                b_occs.len());

            let a_key = cursor.ref_names.iter()
                .find(|(_, n)| n.as_str() == "a")
                .map(|(k, _)| *k);
            assert!(a_key.is_some(), "Expected 'a' in ref_names");
            let a_occs = cursor.ref_groups.get(&a_key.unwrap()).unwrap();
            // At least 2: declaration + this.a argument
            assert!(a_occs.len() >= 2,
                "Expected at least 2 occurrences for 'a' (decl + this.a ref), got {}",
                a_occs.len());
        });
    }

    #[test]
    fn class_this_method_call_token_is_function() {
        // `this.b(...)` — the member `b` should get Function semantic token
        let src = "\
class A {
    void b() {}
    void c() {
        this.b();
    }
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);
            let b_tokens: Vec<_> = tokens.iter()
                .filter(|(t, _)| t == "b")
                .collect();
            // At least one should be Function (the call site)
            assert!(b_tokens.iter().any(|(_, k)| *k == TokenKind::Function),
                "Expected at least one 'b' token as Function (call via this.b()), got: {:?}", b_tokens);
        });
    }

    #[test]
    fn class_this_no_undeclared_diagnostic() {
        // Full example from user: no undeclared diagnostics for bare/this members
        let src = "\
class A {
    int a = 3;
    void b(int d) {
        a = 4;
        this.a = 5;
    }
    void c() {
        b(a);
        this.b(this.a);
    }
}
";
        with_cursor(src, |cursor| {
            // Check no undeclared diagnostics for `a` or `b`
            let undeclared_a: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("\"a\"") || d.message.contains("'a'") || d.message.ends_with(" a"))
                .collect();
            assert!(undeclared_a.is_empty(),
                "Expected no 'undeclared' diagnostic for 'a', got {:?}", undeclared_a);

            let undeclared_b: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("\"b\"") || d.message.contains("'b'") || d.message.ends_with(" b"))
                .collect();
            assert!(undeclared_b.is_empty(),
                "Expected no 'undeclared' diagnostic for 'b', got {:?}", undeclared_b);
        });
    }

    // ─── Forward reference tests (two-pass resolution) ────────────────────

    #[test]
    fn class_forward_method_resolves() {
        // Method `c()` calls `b()` which is declared BELOW `c` — two-pass
        // pre-declaration must make `b` visible inside `c`.
        let src = "\
class A {
    void c() {
        b();
    }
    void b() {}
}
";
        with_cursor(src, |cursor| {
            let b_key = cursor.ref_names.iter()
                .find(|(_, n)| n.as_str() == "b")
                .map(|(k, _)| *k);
            assert!(b_key.is_some(), "Expected 'b' in ref_names");
            let b_occs = cursor.ref_groups.get(&b_key.unwrap()).unwrap();
            // declaration + call site = at least 2
            assert!(b_occs.len() >= 2,
                "Expected at least 2 occurrences for 'b' (decl + forward call), got {}",
                b_occs.len());

            // No undeclared diagnostic for 'b'
            let undecl: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("\"b\"") || d.message.contains("'b'") || d.message.ends_with(" b"))
                .collect();
            assert!(undecl.is_empty(),
                "Expected no 'undeclared' diagnostic for forward method 'b', got {:?}", undecl);
        });
    }

    #[test]
    fn class_forward_property_resolves() {
        // Method `c()` reads property `a` which is declared BELOW `c`.
        let src = "\
class A {
    void c() {
        int x = a;
    }
    int a = 42;
}
";
        with_cursor(src, |cursor| {
            let a_key = cursor.ref_names.iter()
                .find(|(_, n)| n.as_str() == "a")
                .map(|(k, _)| *k);
            assert!(a_key.is_some(), "Expected 'a' in ref_names");
            let a_occs = cursor.ref_groups.get(&a_key.unwrap()).unwrap();
            assert!(a_occs.len() >= 2,
                "Expected at least 2 occurrences for 'a' (decl + forward read), got {}",
                a_occs.len());

            let undecl: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("\"a\"") || d.message.contains("'a'") || d.message.ends_with(" a"))
                .collect();
            assert!(undecl.is_empty(),
                "Expected no 'undeclared' diagnostic for forward property 'a', got {:?}", undecl);
        });
    }

    #[test]
    fn toplevel_forward_function_resolves() {
        // Top-level function `Foo` calls `Bar` declared below it.
        let src = "\
void Foo() {
    Bar();
}
void Bar() {}
";
        with_cursor(src, |cursor| {
            let bar_key = cursor.ref_names.iter()
                .find(|(_, n)| n.as_str() == "Bar")
                .map(|(k, _)| *k);
            assert!(bar_key.is_some(), "Expected 'Bar' in ref_names");
            let bar_occs = cursor.ref_groups.get(&bar_key.unwrap()).unwrap();
            assert!(bar_occs.len() >= 2,
                "Expected at least 2 occurrences for 'Bar' (decl + forward call), got {}",
                bar_occs.len());

            let undecl: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("Bar"))
                .collect();
            assert!(undecl.is_empty(),
                "Expected no 'undeclared' diagnostic for forward function 'Bar', got {:?}", undecl);
        });
    }

    #[test]
    fn toplevel_forward_class_type_resolves() {
        // Function uses class `B` as type before `B` is declared.
        let src = "\
void Foo(B obj) {}
class B {}
";
        with_cursor(src, |cursor| {
            let b_key = cursor.ref_names.iter()
                .find(|(_, n)| n.as_str() == "B")
                .map(|(k, _)| *k);
            assert!(b_key.is_some(), "Expected 'B' in ref_names");
            let b_occs = cursor.ref_groups.get(&b_key.unwrap()).unwrap();
            assert!(b_occs.len() >= 2,
                "Expected at least 2 occurrences for 'B' (decl + type ref), got {}",
                b_occs.len());
        });
    }

    #[test]
    fn forward_class_constructor_call_resolves_as_type() {
        // `MyClass(10)` should resolve to the class constructor (TypeRef),
        // not to a function call, even though MyClass is declared below.
        let src = "\
void Foo() {
    MyClass x = MyClass(10);
}
class MyClass {}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            // The `MyClass` in `MyClass(10)` should be colored as a type.
            let mc_tokens: Vec<_> = tokens.iter()
                .filter(|(t, _)| t == "MyClass")
                .collect();
            // At least one occurrence as Type (the constructor call)
            let has_type = mc_tokens.iter().any(|(_, k)| *k == TokenKind::Type);
            assert!(has_type,
                "Expected at least one 'MyClass' token with Type kind (constructor call), got: {:?}",
                mc_tokens);

            // No undeclared diagnostics for MyClass
            let undecl: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("MyClass"))
                .collect();
            assert!(undecl.is_empty(),
                "Expected no 'undeclared' diagnostics for 'MyClass', got {:?}", undecl);
        });
    }

    #[test]
    fn builtin_type_constructor_call_is_type() {
        // `int(20)` should be colored as a type (built-in constructor).
        let src = "\
void Foo() {
    int x = int(20);
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            // The `int` in `int(20)` should be colored as Type
            let int_tokens: Vec<_> = tokens.iter()
                .filter(|(t, _)| t == "int")
                .collect();
            let has_type = int_tokens.iter().any(|(_, k)| *k == TokenKind::Type);
            assert!(has_type,
                "Expected 'int' in 'int(20)' to be colored as Type, got: {:?}", int_tokens);
        });
    }

    // ─── Cyrillic coordinate tests ──────────────────────────────────────

    #[test]
    fn cyrillic_comment_class_method_tokens() {
        // Cyrillic + emoji in comment — emoji are 4 bytes UTF-8 / 2 code units UTF-16 (surrogate pair),
        // make sure subsequent tokens still have correct UTF-16 coordinates.
        let src = "\
class A {
    //* Привет мир 🔥💎
    int a = 3;
    void b() {}
    void c() {
        this.b();
        this.a = 5;
    }
}
";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            // Method 'b' must be found with Function kind
            let b_tok = tokens.iter().find(|(t, _)| t == "b" );
            assert!(b_tok.is_some(),
                "Expected to find 'b' token after Cyrillic comment, got: {:?}", tokens);
            assert_eq!(b_tok.unwrap().1, TokenKind::Function);

            // Property 'a' must still resolve — no undeclared diagnostics
            let undeclared: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains(" a") || d.message.contains(" b"))
                .collect();
            assert!(undeclared.is_empty(),
                "Expected no 'undeclared' diagnostics with Cyrillic comments, got {:?}", undeclared);
        });
    }

    #[test]
    fn cyrillic_string_same_line_offsets() {
        // Identifier AFTER a Cyrillic + emoji string on the same line.
        // «Привет🔥» = 6×2 + 4 = 16 bytes UTF-8, but 6 + 2 = 8 UTF-16 code units.
        // tree-sitter gives byte offsets; make sure our UTF-16 conversion
        // produces the correct column so the token text matches.
        let src = "string s = \"Привет🔥\"; int a = 0;\n";
        with_cursor(src, |cursor| {
            let tokens = collect_tokens(src, cursor);

            // 'a' should be extracted correctly despite preceding Cyrillic string
            let a_tok = tokens.iter().find(|(t, _)| t == "a");
            assert!(a_tok.is_some(),
                "Expected 'a' token after Cyrillic string literal, tokens: {:?}", tokens);
            assert_eq!(a_tok.unwrap().1, TokenKind::Variable);

            // 's' should also be found
            let s_tok = tokens.iter().find(|(t, _)| t == "s");
            assert!(s_tok.is_some(),
                "Expected 's' token, tokens: {:?}", tokens);
        });
    }

    #[test]
    fn cyrillic_class_method_ref_group_coordinates() {
        // Verify that ref_group occurrence ranges for a method declared after
        // Cyrillic + emoji text (in strings) have correct line/character
        // (UTF-16) positions that round-trip through Position ↔ byte offset.
        // 🔥 = U+1F525 → 4 bytes UTF-8, 2 code units UTF-16 (surrogate pair).
        let src = "\
class A {
    string s = \"Привет🔥мир💎\";
    int a = 3;
    void b() {
        a = 5;
    }
}
";
        with_cursor(src, |cursor| {
            let rope = Rope::from(src);

            // 'a' (property) — declared + referenced inside method
            let a_key = cursor.ref_names.iter()
                .find(|(_, n)| n.as_str() == "a")
                .map(|(k, _)| *k);
            assert!(a_key.is_some(), "Expected 'a' in ref_names, got: {:?}", cursor.ref_names);
            let a_occs = cursor.ref_groups.get(&a_key.unwrap()).unwrap();
            assert!(a_occs.len() >= 2,
                "Expected at least 2 occurrences for 'a', got {}", a_occs.len());

            // Verify that every occurrence's range round-trips through byte offset
            for occ in a_occs {
                let start_byte = occ.range.start.to_byte_offset(&rope);
                assert!(start_byte.is_some(),
                    "Range start {:?} must round-trip to a byte offset", occ.range.start);
                let end_byte = occ.range.end.to_byte_offset(&rope);
                assert!(end_byte.is_some(),
                    "Range end {:?} must round-trip to a byte offset", occ.range.end);

                // The bytes at that offset should be 'a'
                let sb = start_byte.unwrap();
                let eb = end_byte.unwrap();
                let slice = rope.slice_to_cow(sb..eb).to_string();
                assert_eq!(slice, "a",
                    "Expected identifier text 'a' at byte {}..{}, got '{}'", sb, eb, slice);
            }

            // 'b' (method) — should be declared as func
            let b_key = cursor.ref_names.iter()
                .find(|(_, n)| n.as_str() == "b")
                .map(|(k, _)| *k);
            assert!(b_key.is_some(), "Expected 'b' in ref_names");

            // Verify 'b' occurrence also round-trips correctly
            let b_occs = cursor.ref_groups.get(&b_key.unwrap()).unwrap();
            for occ in b_occs {
                let sb = occ.range.start.to_byte_offset(&rope).unwrap();
                let eb = occ.range.end.to_byte_offset(&rope).unwrap();
                let slice = rope.slice_to_cow(sb..eb).to_string();
                assert_eq!(slice, "b",
                    "Expected identifier text 'b' at byte {}..{}, got '{}'", sb, eb, slice);
            }
        });
    }

    #[test]
    fn no_undeclared_int_after_method_call() {
        // Exact user code that reportedly triggers "Undeclared variable `int`"
        let src = "\
void ComputeStatDerived(int heroClass) {
    statDerived.Reset();

    // Тип основного стата из базового шаблона (0=str, 1=agi, 2=int)
    int mainStatType = Jass::R2I(baseStats.mainStat);
}
";
        with_cursor(src, |cursor| {
            let int_diags: Vec<_> = cursor.diagnostics.iter()
                .filter(|d| d.message.contains("int"))
                .collect();
            assert!(int_diags.is_empty(),
                "Expected no diagnostics mentioning 'int', got: {:?}",
                int_diags.iter().map(|d| &d.message).collect::<Vec<_>>());
        });
    }
}
