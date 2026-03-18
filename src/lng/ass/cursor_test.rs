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
        let mut ast = build_ast(tree.root_node());
        let rope = Rope::from(src);
        let src_bytes: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
        rewrite_directives(&mut ast, &src_bytes);
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
        let src = "//import ../common.j\n//set ref-tip 1\n//ignore unused\nvoid main() {}\n";
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

    use crate::lsp::document_symbol::lsp::SymbolKind;

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

    use crate::lsp::folding::lsp::FoldingRangeKind;

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
        let src = "//set ref-tip 1\nvoid main() {}\n";
        with_cursor(src, |cursor| {
            assert_eq!(cursor.file_settings.get("ref-tip").map(|s| s.as_str()),
                Some("1"),
                "Expected file_settings[\"ref-tip\"] = \"1\", got: {:?}", cursor.file_settings);
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
}

