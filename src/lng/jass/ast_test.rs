#[cfg(test)]
mod tests {
    use crate::lng::jass::ast::*;
    use crate::lng::jass::ast::rewrite_imports;
    use tree_sitter::Node;

    fn with_ast(src: &str, f: impl FnOnce(&Ast)) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        f(&ast);
    }

    fn node_text<'a>(src: &'a str, node: &Node) -> &'a str {
        &src[node.start_byte()..node.end_byte()]
    }

    #[test]
    fn type_statement() {
        let src = "type handle extends agent\n";
        with_ast(src, |ast| {
            assert_eq!(ast.items.len(), 1);
            assert!(ast.errors.is_empty());
            match &ast.items[0] {
                Statement::Type(t) => {
                    let name = t.name.as_ref().unwrap();
                    assert_eq!(node_text(src, &name.node), "handle");
                    assert_eq!(name.role, IdRole::TypeDecl);
                    let base = t.base.as_ref().unwrap();
                    assert_eq!(node_text(src, &base.node), "agent");
                    assert_eq!(base.role, IdRole::TypeRef);
                }
                other => panic!("Expected Type, got {:?}", other),
            }
        });
    }

    #[test]
    fn call_args() {
        let src = "call Foo(a, b, c)\n";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Call(c) => {
                    let fc = c.func.as_ref().unwrap();
                    assert_eq!(fc.name.as_ref().unwrap().role, IdRole::FunctionRef);
                    assert_eq!(fc.args.len(), 3);
                    match &fc.args[0] {
                        Expr::Id(id) => assert_eq!(node_text(src, &id.node), "a"),
                        other => panic!("Expected Id, got {:?}", other),
                    }
                }
                other => panic!("Expected Call, got {:?}", other),
            }
        });
    }

    #[test]
    fn local_value_expr() {
        let src = "local integer a = x + 1\n";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Local(l) => {
                    assert!(l.value.is_some());
                    match l.value.as_ref().unwrap() {
                        Expr::Binary { left, right, .. } => {
                            assert!(matches!(left.as_ref(), Expr::Id(_)));
                            assert!(matches!(right.as_ref(), Expr::Literal(_)));
                        }
                        other => panic!("Expected Binary, got {:?}", other),
                    }
                }
                other => panic!("Expected Local, got {:?}", other),
            }
        });
    }

    #[test]
    fn set_index_and_value() {
        let src = "set arr[i] = 5\n";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Set(s) => {
                    assert!(s.index.is_some());
                    match s.index.as_ref().unwrap() {
                        Expr::Id(id) => assert_eq!(node_text(src, &id.node), "i"),
                        other => panic!("Expected Id index, got {:?}", other),
                    }
                    assert!(s.value.is_some());
                }
                other => panic!("Expected Set, got {:?}", other),
            }
        });
    }

    #[test]
    fn return_function_call_expr() {
        let src = "\
function F takes unit t returns boolean
    return UnitLife(t) > 0
endfunction
";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Function(f) => {
                    match &f.body[0] {
                        Statement::Return(r) => {
                            match r.value.as_ref().unwrap() {
                                Expr::Binary { left, .. } => {
                                    match left.as_ref() {
                                        Expr::Call(fc) => {
                                            let name = fc.name.as_ref().unwrap();
                                            assert_eq!(node_text(src, &name.node), "UnitLife");
                                            assert_eq!(name.role, IdRole::FunctionRef);
                                            assert_eq!(fc.args.len(), 1);
                                        }
                                        other => panic!("Expected Call, got {:?}", other),
                                    }
                                }
                                other => panic!("Expected Binary, got {:?}", other),
                            }
                        }
                        other => panic!("Expected Return, got {:?}", other),
                    }
                }
                other => panic!("Expected Function, got {:?}", other),
            }
        });
    }

    #[test]
    fn exitwhen_condition() {
        let src = "exitwhen not b\n";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Exitwhen(e) => {
                    match e.condition.as_ref().unwrap() {
                        Expr::Unary { operand, .. } => {
                            assert!(matches!(operand.as_ref(), Expr::Id(_)));
                        }
                        other => panic!("Expected Unary, got {:?}", other),
                    }
                }
                other => panic!("Expected Exitwhen, got {:?}", other),
            }
        });
    }

    #[test]
    fn if_condition() {
        let src = "\
function F takes nothing returns nothing
    if a > 0 then
        return
    endif
endfunction
";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Function(f) => match &f.body[0] {
                    Statement::If(i) => assert!(matches!(i.condition.as_ref().unwrap(), Expr::Binary { .. })),
                    other => panic!("Expected If, got {:?}", other),
                },
                other => panic!("Expected Function, got {:?}", other),
            }
        });
    }

    #[test]
    fn function_ref_expr() {
        let src = "\
function F takes nothing returns nothing
    return function G
endfunction
";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Function(f) => match &f.body[0] {
                    Statement::Return(r) => match r.value.as_ref().unwrap() {
                        Expr::FuncRef(id) => {
                            assert_eq!(node_text(src, &id.node), "G");
                            assert_eq!(id.role, IdRole::FunctionRef);
                        }
                        other => panic!("Expected FuncRef, got {:?}", other),
                    },
                    other => panic!("Expected Return, got {:?}", other),
                },
                other => panic!("Expected Function, got {:?}", other),
            }
        });
    }

    #[test]
    fn parens_and_index_in_set() {
        let src = "set a = (a * 2) - arr[x]\n";
        with_ast(src, |ast| {
            match &ast.items[0] {
                Statement::Set(s) => {
                    match s.value.as_ref().unwrap() {
                        Expr::Binary { left, right, .. } => {
                            assert!(matches!(left.as_ref(), Expr::Parens { .. }));
                            assert!(matches!(right.as_ref(), Expr::Index { .. }));
                        }
                        other => panic!("Expected Binary, got {:?}", other),
                    }
                }
                other => panic!("Expected Set, got {:?}", other),
            }
        });
    }

    #[test]
    fn collects_errors() {
        let src = "function\n";
        with_ast(src, |ast| {
            assert!(!ast.errors.is_empty());
        });
    }

    // ─── Import directive tests ──────────────────────────────────────────

    fn with_ast_imports(src: &str, f: impl FnOnce(&Ast)) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let mut ast = build_ast(tree.root_node());
        rewrite_imports(&mut ast, src.as_bytes());
        f(&ast);
    }

    #[test]
    fn import_basic() {
        let src = "//import path/to/file.j\nfunction F takes nothing returns nothing\nendfunction\n";
        with_ast_imports(src, |ast| {
            assert_eq!(ast.items.len(), 2);
            match &ast.items[0] {
                Statement::Import(imp) => {
                    assert!(!imp.frozen);
                    assert_eq!(imp.path, "path/to/file.j");
                }
                other => panic!("Expected Import, got {:?}", other),
            }
        });
    }

    #[test]
    fn import_frozen() {
        let src = "//import! path/to/file.j\nfunction F takes nothing returns nothing\nendfunction\n";
        with_ast_imports(src, |ast| {
            match &ast.items[0] {
                Statement::Import(imp) => {
                    assert!(imp.frozen);
                    assert_eq!(imp.path, "path/to/file.j");
                }
                other => panic!("Expected Import, got {:?}", other),
            }
        });
    }

    #[test]
    fn import_empty_path_is_still_import() {
        // `//import` with no path — still becomes Import with empty path (error)
        let src = "//import\nfunction F takes nothing returns nothing\nendfunction\n";
        with_ast_imports(src, |ast| {
            match &ast.items[0] {
                Statement::Import(imp) => {
                    assert!(!imp.frozen);
                    assert_eq!(imp.path, "");
                }
                other => panic!("Expected Import, got {:?}", other),
            }
        });
    }

    #[test]
    fn import_frozen_empty_path() {
        let src = "//import!\nfunction F takes nothing returns nothing\nendfunction\n";
        with_ast_imports(src, |ast| {
            match &ast.items[0] {
                Statement::Import(imp) => {
                    assert!(imp.frozen);
                    assert_eq!(imp.path, "");
                }
                other => panic!("Expected Import, got {:?}", other),
            }
        });
    }

    #[test]
    fn import_stops_at_first_statement() {
        // //import after real code stays as Comment
        let src = "//import a.j\n//import b.j\nfunction F takes nothing returns nothing\nendfunction\n//import c.j\n";
        with_ast_imports(src, |ast| {
            let import_count = ast.items.iter().filter(|s| matches!(s, Statement::Import(_))).count();
            assert_eq!(import_count, 2);
            // "//import c.j" after the function stays as a Comment
            let comment_count = ast.items.iter().filter(|s| matches!(s, Statement::Comment(_))).count();
            assert_eq!(comment_count, 1);
        });
    }

    #[test]
    fn import_after_code_is_comment() {
        // Even a single statement before //import makes it a plain comment
        let src = "type handle extends agent\n//import a.j\n";
        with_ast_imports(src, |ast| {
            let import_count = ast.items.iter().filter(|s| matches!(s, Statement::Import(_))).count();
            assert_eq!(import_count, 0);
            let comment_count = ast.items.iter().filter(|s| matches!(s, Statement::Comment(_))).count();
            assert_eq!(comment_count, 1);
        });
    }

    #[test]
    fn import_not_at_column_zero() {
        // Indented comment should NOT be rewritten to import
        let src = " //import a.j\nfunction F takes nothing returns nothing\nendfunction\n";
        with_ast_imports(src, |ast| {
            let import_count = ast.items.iter().filter(|s| matches!(s, Statement::Import(_))).count();
            assert_eq!(import_count, 0);
        });
    }

    #[test]
    fn import_mixed_with_regular_comments() {
        let src = "//import a.j\n// regular comment\n//import b.j\nfunction F takes nothing returns nothing\nendfunction\n";
        with_ast_imports(src, |ast| {
            let import_count = ast.items.iter().filter(|s| matches!(s, Statement::Import(_))).count();
            assert_eq!(import_count, 2);
            let comment_count = ast.items.iter().filter(|s| matches!(s, Statement::Comment(_))).count();
            assert_eq!(comment_count, 1);
        });
    }

    #[test]
    fn import_no_false_positive() {
        // "//importing" should NOT match (no space/tab after "//import")
        let src = "//importing stuff\nfunction F takes nothing returns nothing\nendfunction\n";
        with_ast_imports(src, |ast| {
            let import_count = ast.items.iter().filter(|s| matches!(s, Statement::Import(_))).count();
            assert_eq!(import_count, 0);
        });
    }

    #[test]
    fn import_code_between_stops_second_import() {
        // `a = 2` between two imports — the second import must stay a comment
        let src = "//import path/to/file\na = 2\n//import! path/to/file\n";
        with_ast_imports(src, |ast| {
            let import_count = ast.items.iter().filter(|s| matches!(s, Statement::Import(_))).count();
            assert_eq!(import_count, 1, "Only the first //import should be recognized");
            let comment_count = ast.items.iter().filter(|s| matches!(s, Statement::Comment(_))).count();
            assert_eq!(comment_count, 1, "The second //import! should stay as Comment");
        });
    }

    // ─── //set directive tests ──────────────────────────────────────────

    #[test]
    fn set_directive_basic() {
        let src = "//set hint ref\nfunction F takes nothing returns nothing\nendfunction\n";
        with_ast_imports(src, |ast| {
            let set_count = ast.items.iter().filter(|s| matches!(s, Statement::SetDir(_))).count();
            assert_eq!(set_count, 1);
            if let Statement::SetDir(sd) = &ast.items[0] {
                assert_eq!(sd.key, "hint");
                assert_eq!(sd.value, "ref");
            } else {
                panic!("expected SetDir");
            }
        });
    }

    #[test]
    fn set_directive_empty_value() {
        let src = "//set hint\nfunction F takes nothing returns nothing\nendfunction\n";
        with_ast_imports(src, |ast| {
            let set_count = ast.items.iter().filter(|s| matches!(s, Statement::SetDir(_))).count();
            assert_eq!(set_count, 1);
            if let Statement::SetDir(sd) = &ast.items[0] {
                assert_eq!(sd.key, "hint");
                assert_eq!(sd.value, "");
            } else {
                panic!("expected SetDir");
            }
        });
    }

    #[test]
    fn set_directive_empty_key() {
        let src = "//set\nfunction F takes nothing returns nothing\nendfunction\n";
        with_ast_imports(src, |ast| {
            let set_count = ast.items.iter().filter(|s| matches!(s, Statement::SetDir(_))).count();
            assert_eq!(set_count, 1);
            if let Statement::SetDir(sd) = &ast.items[0] {
                assert_eq!(sd.key, "");
                assert_eq!(sd.value, "");
            } else {
                panic!("expected SetDir");
            }
        });
    }

    #[test]
    fn set_and_import_interleaved() {
        let src = "//import path/to/file\n//set hint ref\n//import! other.j\nfunction F takes nothing returns nothing\nendfunction\n";
        with_ast_imports(src, |ast| {
            let import_count = ast.items.iter().filter(|s| matches!(s, Statement::Import(_))).count();
            let set_count = ast.items.iter().filter(|s| matches!(s, Statement::SetDir(_))).count();
            assert_eq!(import_count, 2);
            assert_eq!(set_count, 1);
        });
    }

    #[test]
    fn set_after_code_stays_comment() {
        let src = "function F takes nothing returns nothing\nendfunction\n//set hint ref\n";
        with_ast_imports(src, |ast| {
            let set_count = ast.items.iter().filter(|s| matches!(s, Statement::SetDir(_))).count();
            assert_eq!(set_count, 0, "//set after code should stay as comment");
        });
    }

    #[test]
    fn set_no_false_positive() {
        // "//setting" should NOT match
        let src = "//setting stuff\nfunction F takes nothing returns nothing\nendfunction\n";
        with_ast_imports(src, |ast| {
            let set_count = ast.items.iter().filter(|s| matches!(s, Statement::SetDir(_))).count();
            assert_eq!(set_count, 0);
        });
    }

    #[test]
    fn native_and_constant_native_are_statement_native() {
        let src = "native Foo takes nothing returns nothing\nconstant native Bar takes integer a returns integer\n";
        with_ast(src, |ast| {
            assert_eq!(ast.items.len(), 2, "Expected 2 items, got {:?}", ast.items);
            assert!(
                matches!(&ast.items[0], Statement::Native(_)),
                "Expected Statement::Native for 'native Foo', got {:?}",
                ast.items[0]
            );
            assert!(
                matches!(&ast.items[1], Statement::Native(_)),
                "Expected Statement::Native for 'constant native Bar', got {:?}",
                ast.items[1]
            );
        });
    }

    #[test]
    fn type_is_statement_type() {
        let src = "type agent extends handle\n";
        with_ast(src, |ast| {
            assert_eq!(ast.items.len(), 1);
            assert!(matches!(&ast.items[0], Statement::Type(_)));
        });
    }

    #[test]
    fn natives_excluded_from_function_list() {
        // A file with natives, types, and functions — only functions should remain
        let src = "type agent extends handle\nnative Foo takes nothing returns nothing\nconstant native Bar takes integer a returns integer\nfunction Baz takes nothing returns nothing\nendfunction\n";
        with_ast(src, |ast| {
            let funcs: Vec<_> = ast.items.iter().filter(|s| matches!(s, Statement::Function(_))).collect();
            let natives: Vec<_> = ast.items.iter().filter(|s| matches!(s, Statement::Native(_))).collect();
            let types: Vec<_> = ast.items.iter().filter(|s| matches!(s, Statement::Type(_))).collect();
            assert_eq!(funcs.len(), 1, "Expected 1 function");
            assert_eq!(natives.len(), 2, "Expected 2 natives");
            assert_eq!(types.len(), 1, "Expected 1 type");
        });
    }

    #[test]
    fn common_j_style_natives_parsed_correctly() {
        // common.j uses lots of whitespace in native declarations
        let src = "\
type agent                          extends     handle
type event              extends     agent
type player             extends     agent

constant native ConvertRace                 takes integer i returns race
constant native ConvertAllianceType         takes integer i returns alliancetype
native CreateUnit                   takes player id, integer unitid, real x, real y, real face returns unit
constant native GetHandleId takes handle h returns integer

function InitBlizzard takes nothing returns nothing
endfunction
";
        with_ast(src, |ast| {
            let types: Vec<_> = ast.items.iter().filter(|s| matches!(s, Statement::Type(_))).collect();
            let natives: Vec<_> = ast.items.iter().filter(|s| matches!(s, Statement::Native(_))).collect();
            let funcs: Vec<_> = ast.items.iter().filter(|s| matches!(s, Statement::Function(_))).collect();

            assert_eq!(types.len(), 3, "Expected 3 types, got {}", types.len());
            assert_eq!(natives.len(), 4, "Expected 4 natives, got {}", natives.len());
            assert_eq!(funcs.len(), 1, "Expected 1 function, got {}", funcs.len());

            // Verify no natives or types are accidentally parsed as functions
            for item in &ast.items {
                if let Statement::Function(f) = item {
                    let name = f.name.as_ref().map(|id| node_text(src, &id.node)).unwrap_or("");
                    assert_eq!(name, "InitBlizzard", "Only InitBlizzard should be a function, got: {}", name);
                }
            }
        });
    }

    #[test]
    fn build_fragments_skip_natives_and_types() {
        // Simulates what collect_fragments does: parse file, iterate AST, skip native/type
        let src = "\
type agent extends handle
native Foo takes nothing returns nothing
constant native Bar takes integer a returns integer
function Baz takes nothing returns nothing
endfunction
";
        with_ast(src, |ast| {
            let mut forward_decls = Vec::<String>::new();
            for item in &ast.items {
                match item {
                    Statement::Type(_) | Statement::Native(_) => {
                        // These should be skipped — exactly what build.rs does
                    }
                    Statement::Function(f) => {
                        let name = f.name.as_ref().map(|id| node_text(src, &id.node)).unwrap_or("");
                        forward_decls.push(format!("function {} takes ...", name));
                    }
                    _ => {}
                }
            }
            assert_eq!(forward_decls.len(), 1, "Only 1 function forward decl expected");
            assert!(forward_decls[0].contains("Baz"), "Expected Baz, got: {}", forward_decls[0]);
        });
    }

    /// Parse real common.j and show the actual breakdown of statement types.
    /// This test never fails but prints the stats so we can see what's happening.
    #[test]
    fn real_common_j_parse_stats() {
        let path = "/Users/nazarpunk/Downloads/JassTest/Scripts/common.j";
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("Skipping real_common_j_parse_stats: file not found");
                return;
            }
        };
        with_ast(&src, |ast| {
            let mut natives = 0usize;
            let mut types = 0usize;
            let mut funcs = 0usize;
            let mut globals = 0usize;
            let mut comments = 0usize;
            let mut errors = 0usize;
            let mut leaked_func_names = Vec::<String>::new();

            for item in &ast.items {
                match item {
                    Statement::Native(_) => natives += 1,
                    Statement::Type(_) => types += 1,
                    Statement::Function(f) => {
                        funcs += 1;
                        let name = f.name.as_ref()
                            .map(|id| node_text(&src, &id.node))
                            .unwrap_or("<unnamed>");
                        leaked_func_names.push(name.to_string());
                    }
                    Statement::Globals(_) => globals += 1,
                    Statement::Comment(_) => comments += 1,
                    Statement::Error(_) => errors += 1,
                    _ => {}
                }
            }
            eprintln!("=== common.j AST breakdown ===");
            eprintln!("  natives:   {}", natives);
            eprintln!("  types:     {}", types);
            eprintln!("  functions: {}", funcs);
            eprintln!("  globals:   {}", globals);
            eprintln!("  comments:  {}", comments);
            eprintln!("  errors:    {}", errors);
            if !leaked_func_names.is_empty() {
                eprintln!("  LEAKED FUNCTION NAMES: {:?}", leaked_func_names);
            }
            // common.j should NOT have any function_statement nodes
            assert_eq!(funcs, 0, "common.j should have 0 functions, but found: {:?}", leaked_func_names);
        });
    }

    /// Parse real Blizzard.j and show stats.
    #[test]
    fn real_blizzard_j_parse_stats() {
        let path = "/Users/nazarpunk/Downloads/JassTest/Scripts/Blizzard.j";
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("Skipping real_blizzard_j_parse_stats: file not found");
                return;
            }
        };
        with_ast(&src, |ast| {
            let mut natives = 0usize;
            let mut types = 0usize;
            let mut funcs = 0usize;
            let mut globals = 0usize;
            let mut leaked_native_names = Vec::<String>::new();

            for item in &ast.items {
                match item {
                    Statement::Native(n) => {
                        natives += 1;
                        let name = n.name.as_ref()
                            .map(|id| node_text(&src, &id.node))
                            .unwrap_or("<unnamed>");
                        leaked_native_names.push(name.to_string());
                    }
                    Statement::Type(_) => types += 1,
                    Statement::Function(_) => funcs += 1,
                    Statement::Globals(_) => globals += 1,
                    _ => {}
                }
            }
            eprintln!("=== Blizzard.j AST breakdown ===");
            eprintln!("  natives:   {}", natives);
            eprintln!("  types:     {}", types);
            eprintln!("  functions: {}", funcs);
            eprintln!("  globals:   {}", globals);
            if !leaked_native_names.is_empty() {
                eprintln!("  NATIVE NAMES (should be 0): {:?}", leaked_native_names);
            }
            // Blizzard.j should NOT have any native_statement nodes
            assert_eq!(natives, 0, "Blizzard.j should have 0 natives, but found: {:?}", leaked_native_names);
        });
    }
}

