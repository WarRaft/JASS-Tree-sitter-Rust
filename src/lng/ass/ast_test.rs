#[cfg(test)]
mod tests {
    use crate::lng::ass::ast::*;
    use tree_sitter::Node;

    fn with_ast(src: &str, f: impl FnOnce(&Ast)) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_as::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        f(&ast);
    }

    fn node_text<'a>(src: &'a str, node: &Node) -> &'a str {
        &src[node.start_byte()..node.end_byte()]
    }

    #[test]
    fn function_with_local_var_type() {
        let src = "\
int CountUnitInGroupOfPlayer(player p, int id) {
    group g = CreateGroup();
}
";
        with_ast(src, |ast| {
            assert!(ast.errors.is_empty(), "errors: {:?}", ast.errors);
            assert_eq!(ast.items.len(), 1);
            match &ast.items[0] {
                TopLevel::Function(f) => {
                    let name = f.name.as_ref().unwrap();
                    assert_eq!(node_text(src, &name.node), "CountUnitInGroupOfPlayer");
                    assert_eq!(name.role, IdRole::FunctionDecl);

                    let ret = f.return_type.as_ref().unwrap();
                    assert_eq!(ret.role, IdRole::TypeRef);
                    assert_eq!(node_text(src, &ret.node), "int");

                    assert_eq!(f.params.len(), 2);
                    let p0 = &f.params[0];
                    let p0_type = p0.type_id.as_ref().unwrap();
                    assert_eq!(p0_type.role, IdRole::TypeRef);
                    let p0_name = p0.name.as_ref().unwrap();
                    assert_eq!(node_text(src, &p0_name.node), "p");
                    assert_eq!(p0_name.role, IdRole::Param);

                    let var_decls: Vec<_> = f.body.iter().filter(|s| matches!(s, Stmt::VarDecl(_))).collect();
                    assert_eq!(var_decls.len(), 1, "body: {:?}", f.body);
                    match &var_decls[0] {
                        Stmt::VarDecl(v) => {
                            let type_id = v.type_id.as_ref().unwrap();
                            assert_eq!(type_id.role, IdRole::TypeRef);

                            assert_eq!(v.decls.len(), 1);
                            let d = &v.decls[0];
                            let dn = d.name.as_ref().unwrap();
                            assert_eq!(node_text(src, &dn.node), "g");
                            assert_eq!(dn.role, IdRole::Variable);

                            match d.value.as_ref().unwrap() {
                                Expr::Call { callee, .. } => {
                                    let callee = callee.as_ref().unwrap();
                                    assert_eq!(callee.role, IdRole::FunctionCall);
                                }
                                other => panic!("Expected Call, got {:?}", other),
                            }
                        }
                        other => panic!("Expected VarDecl, got {:?}", other),
                    }
                }
                other => panic!("Expected Function, got {:?}", other),
            }
        });
    }

    #[test]
    fn class_with_method() {
        let src = "\
class Foo {
    int bar() {
        return 42;
    }
}
";
        with_ast(src, |ast| {
            assert!(ast.errors.is_empty(), "errors: {:?}", ast.errors);
            match &ast.items[0] {
                TopLevel::Class(c) => {
                    let name = c.name.as_ref().unwrap();
                    assert_eq!(node_text(src, &name.node), "Foo");
                    assert_eq!(name.role, IdRole::ClassDecl);
                    assert_eq!(c.members.len(), 1);
                    match &c.members[0] {
                        ClassMember::Function(f) => {
                            let fn_name = f.name.as_ref().unwrap();
                            assert_eq!(node_text(src, &fn_name.node), "bar");
                            assert_eq!(fn_name.role, IdRole::FunctionDecl);
                        }
                        other => panic!("Expected Function member, got {:?}", other),
                    }
                }
                other => panic!("Expected Class, got {:?}", other),
            }
        });
    }

    #[test]
    fn enum_members() {
        let src = "\
enum Color {
    Red,
    Green = 1,
    Blue
}
";
        with_ast(src, |ast| {
            assert!(ast.errors.is_empty(), "errors: {:?}", ast.errors);
            match &ast.items[0] {
                TopLevel::Enum(e) => {
                    let name = e.name.as_ref().unwrap();
                    assert_eq!(node_text(src, &name.node), "Color");
                    assert_eq!(name.role, IdRole::EnumDecl);
                    assert_eq!(e.members.len(), 3);
                    let m0 = &e.members[0];
                    assert_eq!(node_text(src, &m0.name.as_ref().unwrap().node), "Red");
                    assert_eq!(m0.name.as_ref().unwrap().role, IdRole::EnumMember);
                }
                other => panic!("Expected Enum, got {:?}", other),
            }
        });
    }

    #[test]
    fn var_decl_after_method_call_with_comment() {
        // Regression: a comment between a method call and a variable declaration
        // caused tree-sitter to misparse the variable declaration as an expression
        // statement, producing "Undeclared variable `int`" diagnostics.
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_as::language().into())
            .expect("Failed to set language");

        let cases = [
            ("no comment", "\
void f(int heroClass) {
    statDerived.Reset();
    int mainStatType = 0;
}
"),
            ("blank line", "\
void f(int heroClass) {
    statDerived.Reset();

    int mainStatType = 0;
}
"),
            ("simple comment", "\
void f(int heroClass) {
    statDerived.Reset();
    // hello
    int mainStatType = 0;
}
"),
            ("ASCII comment with parens", "\
void f(int heroClass) {
    statDerived.Reset();
    // Main stat type (0=str, 1=agi, 2=int)
    int mainStatType = 0;
}
"),
            ("Cyrillic comment", "\
void f(int heroClass) {
    statDerived.Reset();

    // Тип основного стата из базового шаблона (0=str, 1=agi, 2=int)
    int mainStatType = Jass::R2I(baseStats.mainStat);
}
"),
        ];

        for (label, src) in &cases {
            let tree = parser.parse(src, None).expect("Failed to parse");
            let sexp = tree.root_node().to_sexp();
            assert!(sexp.contains("variable_declaration_statement"),
                "Case '{}': expected variable_declaration_statement in CST", label);

            let ast = build_ast(tree.root_node());
            assert!(ast.errors.is_empty(),
                "Case '{}': unexpected errors: {:?}", label, ast.errors);

            match &ast.items[0] {
                TopLevel::Function(f) => {
                    let var_decls: Vec<_> = f.body.iter()
                        .filter(|s| matches!(s, Stmt::VarDecl(_)))
                        .collect();
                    assert_eq!(var_decls.len(), 1,
                        "Case '{}': expected exactly 1 VarDecl in body, got {}",
                        label, var_decls.len());
                }
                other => panic!("Case '{}': expected Function, got {:?}", label, other),
            }
        }
    }
}
