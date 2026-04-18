#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::lng::jass::ast::{build_ast, Statement};
    use std::collections::HashSet;

    fn first_function_src(src: &str) -> String {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        let func = ast
            .items
            .iter()
            .find_map(|item| match item {
                Statement::Function(f) => Some(f),
                _ => None,
            })
            .expect("expected function");
        render_function(src, func).0
    }

    #[test]
    fn hoists_late_local_to_top_and_rewrites_initializer() {
        let src = "function A takes nothing returns nothing\n    call Foo()\n    local unit u = CreateUnit()\n    call Bar(u)\nendfunction\n";
        let out = first_function_src(src);

        assert!(out.contains("function A takes nothing returns nothing\n    local unit u = null\n    call Foo()\n    set u = CreateUnit()\n    call Bar(u)\n    set u = null\nendfunction"));
    }

    #[test]
    fn hoists_nested_local_from_if_block() {
        let src = "function A takes nothing returns nothing\n    if cond then\n        local integer x = 5\n        call Foo(x)\n    endif\nendfunction\n";
        let out = first_function_src(src);

        assert!(out.contains("function A takes nothing returns nothing\n    local integer x = 0\n    if cond then\n        set x = 5\n        call Foo(x)\n    endif\nendfunction"));
    }

    #[test]
    fn hoists_varstmt_inside_function_body() {
        let src = "function A takes nothing returns nothing\n    call Foo()\n    integer x = 5\n    call Bar(x)\nendfunction\n";
        let out = first_function_src(src);

        assert!(out.contains("function A takes nothing returns nothing\n    local integer x = 0\n    call Foo()\n    set x = 5\n    call Bar(x)\nendfunction"));
    }

    #[test]
    fn inserts_leak_fix_before_fallthrough_endfunction() {
        let src = "function A takes nothing returns nothing\n    local unit u = CreateUnit()\n    call Bar(u)\nendfunction\n";
        let out = first_function_src(src);

        assert!(out.contains("    local unit u = CreateUnit()\n    call Bar(u)\n    set u = null\nendfunction"));
    }

    #[test]
    fn inserts_leak_fix_before_return() {
        let src = "function A takes nothing returns nothing\n    local unit u = CreateUnit()\n    return\nendfunction\n";
        let out = first_function_src(src);

        assert!(out.contains("    local unit u = CreateUnit()\n    set u = null\n    return\nendfunction"));
    }

    #[test]
    fn rewrites_returned_handle_local_via_temp_local() {
        let src = "function A takes nothing returns unit\n    local unit u = CreateUnit()\n    return u\nendfunction\n";
        let (out, globals) = {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&tree_sitter_jass::language().into())
                .expect("Failed to set language");
            let tree = parser.parse(src, None).expect("Failed to parse");
            let ast = build_ast(tree.root_node());
            let func = ast
                .items
                .iter()
                .find_map(|item| match item {
                    Statement::Function(f) => Some(f),
                    _ => None,
                })
                .expect("expected function");
            render_function(src, func)
        };

        // return temp is a global, not a local inside the function
        assert!(globals.iter().any(|g| g.contains("A_ret")), "expected A_ret in globals: {:?}", globals);
        assert!(!out.contains("local unit A_ret"), "A_ret must not be a local: {}", out);
        assert!(out.contains(
            "function A takes nothing returns unit\n    local unit u = CreateUnit()\n    set A_ret = u\n    set u = null\n    return A_ret\nendfunction"
        ));
    }

    #[test]
    fn return_temp_avoids_function_name_collision() {
        // `Cunt_ret` is the name of another function — the generated temp must skip it.
        // `Cunt_ret1` is a global variable — must skip that too.
        // Expected result: `Cunt_ret2`.
        let src = "function Cunt takes nothing returns unit\n    local unit A = GetTriggerUnit()\n    return A\nendfunction\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        let func = ast
            .items
            .iter()
            .find_map(|item| match item {
                Statement::Function(f) => Some(f),
                _ => None,
            })
            .expect("expected function");

        // Cunt_ret  — function name (collides)
        // Cunt_ret1 — global variable (collides)
        let reserved = HashSet::from(["Cunt_ret".to_string(), "Cunt_ret1".to_string()]);
        let (out, globals) = render_function_with_reserved(src, func, &reserved);

        assert!(
            globals.iter().any(|g| g.contains("Cunt_ret2")),
            "expected Cunt_ret2 in globals (Cunt_ret and Cunt_ret1 are taken): {:?}",
            globals
        );
        assert!(
            out.contains("set Cunt_ret2 = A") && out.contains("return Cunt_ret2"),
            "expected return to use Cunt_ret2: {}",
            out
        );
    }

    #[test]
    fn reserved_function_names_rename_shadowing_user_local_decls() {
        let src = "function Cunt_ret takes nothing returns nothing\n    local integer Cunt_ret = 1\nendfunction\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        let func = ast
            .items
            .iter()
            .find_map(|item| match item {
                Statement::Function(f) => Some(f),
                _ => None,
            })
            .expect("expected function");

        let reserved = HashSet::from(["Cunt_ret".to_string()]);
        let (out, _globals) = render_function_with_reserved(src, func, &reserved);

        assert!(
            out.contains("local integer Cunt_ret1 = 1"),
            "shadowing local declaration must be renamed: {}",
            out
        );
    }

    #[test]
    fn return_temp_avoids_reserved_global_name_collision() {
        let src = "function Cunt takes nothing returns unit\n    local unit A = GetTriggerUnit()\n    return A\nendfunction\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        let func = ast
            .items
            .iter()
            .find_map(|item| match item {
                Statement::Function(f) => Some(f),
                _ => None,
            })
            .expect("expected function");

        let reserved = HashSet::from(["Cunt_ret".to_string()]);
        let (out, globals) = render_function_with_reserved(src, func, &reserved);

        assert!(
            globals.iter().any(|g| g.contains("Cunt_ret1")),
            "expected Cunt_ret1 in globals: {:?}",
            globals
        );
        assert!(
            out.contains("set Cunt_ret1 = A") && out.contains("return Cunt_ret1"),
            "expected rewritten return to use Cunt_ret1: {}",
            out
        );
    }

    #[test]
    fn return_temp_only_collides_on_exact_case() {
        let src = "function Cunt takes nothing returns unit\n    local unit A = GetTriggerUnit()\n    return A\nendfunction\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        let func = ast
            .items
            .iter()
            .find_map(|item| match item {
                Statement::Function(f) => Some(f),
                _ => None,
            })
            .expect("expected function");

        let reserved = HashSet::from(["cUnT_ReT".to_string()]);
        let (out, globals) = render_function_with_reserved(src, func, &reserved);

        assert!(
            globals.iter().any(|g| g.contains("Cunt_ret = null")),
            "expected exact-case matching only, globals: {:?}",
            globals
        );
        assert!(out.contains("return Cunt_ret"), "{}", out);
    }

    #[test]
    fn rewrites_return_expression_that_uses_live_handle_local() {
        let src = "function A takes nothing returns integer\n    local unit u = CreateUnit()\n    return GetHandleId(u)\nendfunction\n";
        let out = first_function_src(src);

        assert!(out.contains(
            "function A takes nothing returns integer\n    local integer A_ret = 0\n    local unit u = CreateUnit()\n    set A_ret = GetHandleId(u)\n    set u = null\n    return A_ret\nendfunction"
        ));
    }

    #[test]
    fn synth_main_hoists_bare_locals() {
        let src = "local integer x = 5\ncall Foo(x)\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());

        let out = render_main_from_statements(src, &ast.items);
        assert!(out.contains("function main takes nothing returns nothing\n    local integer x = 0\n    set x = 5\n    call Foo(x)\nendfunction"));
    }

    #[test]
    fn synth_main_appends_leak_fix_for_bare_handle_local() {
        let src = "local unit u = CreateUnit()\ncall Foo(u)\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());

        let out = render_main_from_statements(src, &ast.items);
        assert!(out.contains(
            "function main takes nothing returns nothing\n    local unit u = null\n    set u = CreateUnit()\n    call Foo(u)\n    set u = null\nendfunction"
        ));
    }

    #[test]
    fn complex_name_resolution_exact_case() {
        let src = "function Cunt takes nothing returns unit\n    local unit A = GetTriggerUnit()\n    set A = Cunt_ret()\n    return A\nendfunction\n";
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        let func = ast
            .items
            .iter()
            .find_map(|item| match item {
                Statement::Function(f) => Some(f),
                _ => None,
            })
            .expect("expected function");

        let reserved = HashSet::from(["Cunt_ret".to_string()]);
        let (out, globals) = render_function_with_reserved(src, func, &reserved);

        assert!(
            globals.iter().any(|g| g.contains("Cunt_ret1")),
            "expected Cunt_ret1 in globals: {:?}",
            globals
        );
        assert!(
            out.contains("set Cunt_ret1 = A") && out.contains("return Cunt_ret1"),
            "expected rewritten return to use Cunt_ret1: {}",
            out
        );
    }

    #[test]
    fn collision_with_function_and_global_variable() {
        let src = r#"function Cunt_ret takes nothing returns unit
    local integer Cunt_ret = 1
endfunction

function Cunt takes nothing returns unit
    local integer Cunt = 3
    local unit A = GetTriggerUnit()
    set A = Cunt_ret()
    return A
endfunction

integer Cunt_ret = 3
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());

        // Collect all top-level reserved names with exact case.
        let mut reserved = HashSet::new();
        for item in &ast.items {
            match item {
                Statement::Function(f) => {
                    if let Some(id) = &f.name {
                        reserved.insert(id_str(src, id).to_string());
                    }
                }
                Statement::VarStmt(v) => {
                    for decl in &v.decls {
                        if let Some(id) = &decl.name {
                            reserved.insert(id_str(src, id).to_string());
                        }
                    }
                }
                _ => {}
            }
        }

        // Verify that both function name and global variable are in reserved set
        assert!(
            reserved.contains("Cunt_ret"),
            "expected Cunt_ret to be in reserved names: {:?}",
            reserved
        );

        // Find function Cunt
        let func_cunt = ast
            .items
            .iter()
            .find_map(|item| match item {
                Statement::Function(f) => {
                    if let Some(id) = &f.name {
                        if id_str(src, id) == "Cunt" {
                            Some(f)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .expect("expected function Cunt");

        let (out, globals) = render_function_with_reserved(src, func_cunt, &reserved);

        // Should use Cunt_ret1 because Cunt_ret is reserved
        // The fix ensures that when rendering the function, it checks against all
        // reserved names (which includes the global variable name "Cunt_ret")
        // so it generates Cunt_ret1 instead
        assert!(
            globals.iter().any(|g| g.contains("Cunt_ret1")),
            "expected Cunt_ret1 in globals (Cunt_ret is taken by function/global): {:?}",
            globals
        );
        assert!(
            out.contains("set Cunt_ret1 = A") && out.contains("return Cunt_ret1"),
            "expected rewritten return to use Cunt_ret1: {}",
            out
        );
    }
}
