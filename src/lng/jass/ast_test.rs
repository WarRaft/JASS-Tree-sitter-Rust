#[cfg(test)]
mod tests {
    use crate::lng::jass::ast::*;
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
}

