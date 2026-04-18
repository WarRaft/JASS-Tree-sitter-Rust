#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::lng::jass::ast::{build_ast, Statement};

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
}

