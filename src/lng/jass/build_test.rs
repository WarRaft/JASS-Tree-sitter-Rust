#[cfg(test)]
mod tests {
    use crate::lng::jass::ast::{build_ast, rewrite_imports};
    use crate::lng::jass::build::*;

    /// Parse JASS source → AST → emit_function → hoist_jass_locals → final text.
    /// This mirrors the real build pipeline for a single function.
    fn build_function(src: &str) -> String {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let mut ast = build_ast(tree.root_node());
        let src_bytes = src.as_bytes().to_vec();
        rewrite_imports(&mut ast, &src_bytes);

        use crate::lng::jass::ast::Statement;
        let func = ast
            .items
            .iter()
            .find_map(|item| match item {
                Statement::Function(f) => Some(f),
                _ => None,
            })
            .expect("No function found in source");

        let emitted = emit_function_text(src, func);
        hoist_jass_locals_text(&emitted)
    }

    /// Normalize whitespace: trim each line, drop empty lines, join with `\n`.
    fn norm(s: &str) -> String {
        s.lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    // ── Basic: no hoisting needed ────────────────────────────────────────────

    #[test]
    fn locals_at_top_unchanged() {
        let src = "\
function A takes nothing returns nothing
    local integer x = 1
    local real y = 2.0
    set x = 3
endfunction
";
        let result = build_function(src);
        assert_eq!(
            norm(&result),
            norm(
                "function A takes nothing returns nothing\n\
                 local integer x = 1\n\
                 local real y = 2.0\n\
                 set x = 3\n\
                 endfunction"
            )
        );
    }

    // ── Late local gets hoisted ──────────────────────────────────────────────

    #[test]
    fn late_local_hoisted() {
        let src = "\
function A takes nothing returns nothing
    local integer x = 1
    set x = 2
    local real y = 3.0
endfunction
";
        let result = build_function(src);
        let lines: Vec<&str> = result.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
        // Hoisted `y` must appear before `set x = 2`.
        assert!(lines.contains(&"local real y = 0"), "hoisted `local real y = 0` missing: {lines:?}");
        // Original site must become `set y = 3.0`.
        assert!(lines.contains(&"set y = 3.0"), "`set y = 3.0` missing: {lines:?}");
        // Must NOT have a second `local real y = 3.0`.
        let local_y_count = lines.iter().filter(|l| l.starts_with("local real y")).count();
        assert_eq!(local_y_count, 1, "should be exactly one `local real y`, got {local_y_count}: {lines:?}");
    }

    // ── Duplicate locals: same name declared multiple times ──────────────────

    #[test]
    fn duplicate_locals_deduped() {
        let src = "\
function A takes nothing returns nothing
    integer A = 33

    loop
        real B = 33
        exitwhen B < 0
        B = B + 1
    endloop
    real B = 33
    real B

    A = 21
endfunction
";
        let result = build_function(src);
        let lines: Vec<&str> = result.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
        // `B` must be declared exactly once.
        let local_b_count = lines.iter().filter(|l| l.starts_with("local real B")).count();
        assert_eq!(local_b_count, 1, "expected exactly 1 `local real B`, got {local_b_count}: {lines:?}");
        // The hoisted declaration must use the default value.
        assert!(lines.contains(&"local real B = 0"), "hoisted `local real B = 0` missing: {lines:?}");
        // `set A = 21` must be present.
        assert!(lines.contains(&"set A = 21"), "`set A = 21` missing: {lines:?}");
    }

    // ── Early + late same name: early wins, no duplicate hoist ───────────────

    #[test]
    fn early_decl_prevents_duplicate_hoist() {
        let src = "\
function A takes nothing returns nothing
    local integer x = 1
    set x = 2
    integer x = 99
endfunction
";
        let result = build_function(src);
        let lines: Vec<&str> = result.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
        // `x` was already declared early — no hoisted duplicate.
        let local_x_count = lines.iter().filter(|l| l.starts_with("local integer x")).count();
        assert_eq!(local_x_count, 1, "expected exactly 1 `local integer x`, got {local_x_count}: {lines:?}");
        // Late decl with initializer becomes `set x = 99`.
        assert!(lines.contains(&"set x = 99"), "`set x = 99` missing: {lines:?}");
    }

    // ── VarStmt without `local` keyword gets `local` prefix ─────────────────

    #[test]
    fn varstmt_gets_local_prefix() {
        let src = "\
function A takes nothing returns nothing
    integer x = 5
endfunction
";
        let result = build_function(src);
        let lines: Vec<&str> = result.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
        assert!(
            lines.contains(&"local integer x = 5"),
            "expected `local integer x = 5` but got: {lines:?}"
        );
    }

    // ── Loop-scoped late local hoisted once ──────────────────────────────────

    #[test]
    fn loop_local_hoisted_once() {
        let src = "\
function A takes nothing returns nothing
    call Foo()
    loop
        integer i = 0
        exitwhen i > 10
    endloop
endfunction
";
        let result = build_function(src);
        let lines: Vec<&str> = result.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
        let local_i_count = lines.iter().filter(|l| l.starts_with("local integer i")).count();
        assert_eq!(local_i_count, 1, "expected exactly 1 `local integer i`, got {local_i_count}: {lines:?}");
        assert!(lines.contains(&"local integer i = 0"), "hoisted `local integer i = 0` missing: {lines:?}");
        assert!(lines.contains(&"set i = 0"), "`set i = 0` at original site missing: {lines:?}");
    }

    // ── No-initializer late local: no set emitted ────────────────────────────

    #[test]
    fn late_local_no_init_no_set() {
        let src = "\
function A takes nothing returns nothing
    call Foo()
    real x
endfunction
";
        let result = build_function(src);
        let lines: Vec<&str> = result.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
        assert!(lines.contains(&"local real x = 0"), "hoisted `local real x = 0` missing: {lines:?}");
        // No `set x = ...` because original had no initializer.
        let set_x = lines.iter().any(|l| l.starts_with("set x"));
        assert!(!set_x, "unexpected `set x` for uninitialised decl: {lines:?}");
    }

    // ── Array local hoisted without default value ────────────────────────────

    #[test]
    fn array_local_hoisted() {
        let src = "\
function A takes nothing returns nothing
    call Foo()
    integer array arr
endfunction
";
        let result = build_function(src);
        let lines: Vec<&str> = result.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
        assert!(
            lines.contains(&"local integer array arr"),
            "hoisted `local integer array arr` missing: {lines:?}"
        );
    }

    // ── AS precedence: `or` inside `and` gets parenthesized ─────────────────

    /// Helper: parse a global var decl and emit it in AS mode.
    fn emit_global_var_as(src: &str) -> String {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let mut ast = build_ast(tree.root_node());
        let src_bytes = src.as_bytes().to_vec();
        rewrite_imports(&mut ast, &src_bytes);

        use crate::lng::jass::ast::Statement;
        let var = ast
            .items
            .iter()
            .find_map(|item| match item {
                Statement::VarStmt(v) => Some(v),
                _ => None,
            })
            .expect("No VarStmt found");
        emit_var_text_as(src, var)
    }

    /// Helper: parse source, emit function body in AS mode.
    fn build_function_as(src: &str) -> String {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let mut ast = build_ast(tree.root_node());
        let src_bytes = src.as_bytes().to_vec();
        rewrite_imports(&mut ast, &src_bytes);

        use crate::lng::jass::ast::Statement;
        let func = ast
            .items
            .iter()
            .find_map(|item| match item {
                Statement::Function(f) => Some(f),
                _ => None,
            })
            .expect("No function found");
        emit_function_text_as(src, func)
    }

    #[test]
    fn as_or_inside_and_gets_parens() {
        // In JASS: `false and true or true` means `false and (true or true)`.
        // In AS:   without parens it would mean `(false and true) or true`.
        let src = "boolean T = false and true or true\n";
        let result = emit_global_var_as(src);
        assert!(
            result.contains("(true or true)"),
            "expected `(true or true)` in AS output, got: {result}"
        );
        assert!(
            result.contains("false and (true or true)"),
            "expected `false and (true or true)`, got: {result}"
        );
    }

    #[test]
    fn as_and_inside_or_no_extra_parens() {
        // `a or b and c` in JASS = `(a or b) and c`.
        // The `or` is the child of `and` in the AST, so it gets parens.
        // But `and` inside `or` is fine in AS (and already binds tighter).
        let src = "boolean T = true or false and true\n";
        let result = emit_global_var_as(src);
        // Should NOT double-wrap `and` — it already binds tighter in AS.
        assert!(
            !result.contains("(("),
            "no double parens expected, got: {result}"
        );
    }

    #[test]
    fn as_no_parens_when_no_or() {
        // Pure `and` — no precedence issue.
        let src = "boolean T = true and false and true\n";
        let result = emit_global_var_as(src);
        assert!(
            !result.contains('('),
            "no parens expected for pure `and`, got: {result}"
        );
    }

    #[test]
    fn as_or_precedence_in_function_body() {
        let src = "\
function A takes nothing returns nothing
    local boolean x = false and true or true
endfunction
";
        let result = build_function_as(src);
        assert!(
            result.contains("(true or true)"),
            "expected `(true or true)` in function body, got: {result}"
        );
    }

    #[test]
    fn as_jass_mode_no_parens() {
        // JASS mode: no parentheses added (precedence is correct as-is).
        let src = "function A takes nothing returns nothing\n    local boolean x = false and true or true\nendfunction\n";
        let result = build_function(src);
        assert!(
            !result.contains("(true or true)"),
            "JASS mode should NOT add parens, got: {result}"
        );
    }
}

