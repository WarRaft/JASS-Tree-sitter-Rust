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

    // ─── Function inlining ───────────────────────────────────────────────────

    #[test]
    fn detect_inline_candidate_simple_return() {
        let src = "\
function A takes nothing returns boolean
    return true
endfunction
";
        let result = detect_inline_candidate_text(src);
        assert!(result.is_some(), "should detect inline candidate");
        let (expr, is_compound) = result.unwrap();
        assert_eq!(expr, "true");
        assert!(!is_compound, "literal should not be compound");
    }

    #[test]
    fn detect_inline_candidate_binary_expr() {
        let src = "\
function A takes nothing returns boolean
    return GetUnitUserData(udg_Target) == 74
endfunction
";
        let result = detect_inline_candidate_text(src);
        assert!(result.is_some(), "should detect inline candidate");
        let (expr, is_compound) = result.unwrap();
        assert_eq!(expr, "GetUnitUserData(udg_Target) == 74");
        assert!(is_compound, "binary expression should be compound");
    }

    #[test]
    fn detect_inline_candidate_variable_return() {
        let src = "\
function A takes nothing returns integer
    return udg_X
endfunction
";
        let result = detect_inline_candidate_text(src);
        assert!(result.is_some());
        let (expr, is_compound) = result.unwrap();
        assert_eq!(expr, "udg_X");
        assert!(!is_compound, "variable should not be compound");
    }

    #[test]
    fn detect_inline_candidate_call_return() {
        let src = "\
function A takes nothing returns integer
    return GetUnitState(u)
endfunction
";
        let result = detect_inline_candidate_text(src);
        assert!(result.is_some());
        let (expr, is_compound) = result.unwrap();
        assert_eq!(expr, "GetUnitState(u)");
        assert!(!is_compound, "call should not be compound");
    }

    #[test]
    fn detect_inline_candidate_not_takes_params() {
        let src = "\
function A takes integer x returns boolean
    return x == 5
endfunction
";
        // Should NOT be detected — function takes parameters.
        let result = detect_inline_candidate_text(src);
        assert!(result.is_none(), "function with params should not be candidate");
    }

    #[test]
    fn detect_inline_candidate_not_multiple_stmts() {
        let src = "\
function A takes nothing returns boolean
    local integer x = 5
    return x == 5
endfunction
";
        let result = detect_inline_candidate_text(src);
        assert!(result.is_none(), "function with multiple statements should not be candidate");
    }

    #[test]
    fn detect_inline_candidate_unary_compound() {
        let src = "\
function A takes nothing returns boolean
    return not udg_Flag
endfunction
";
        let result = detect_inline_candidate_text(src);
        assert!(result.is_some());
        let (expr, is_compound) = result.unwrap();
        assert_eq!(expr, "not udg_Flag");
        assert!(is_compound, "unary expression should be compound");
    }

    #[test]
    fn inline_top_level_if_condition() {
        let source = "if MyFunc() then";
        assert!(is_top_level_call_text(source, "MyFunc"), "if COND then should be top-level");
    }

    #[test]
    fn inline_top_level_return() {
        let source = "    return MyFunc()";
        assert!(is_top_level_call_text(source, "MyFunc"), "return EXPR should be top-level");
    }

    #[test]
    fn inline_top_level_call_stmt() {
        let source = "    call MyFunc()";
        assert!(is_top_level_call_text(source, "MyFunc"), "call NAME() should be top-level");
    }

    #[test]
    fn inline_top_level_set() {
        let source = "    set x = MyFunc()";
        assert!(is_top_level_call_text(source, "MyFunc"), "set VAR = NAME() should be top-level");
    }

    #[test]
    fn inline_top_level_exitwhen() {
        let source = "    exitwhen MyFunc()";
        assert!(is_top_level_call_text(source, "MyFunc"), "exitwhen NAME() should be top-level");
    }

    #[test]
    fn inline_nested_in_binary() {
        let source = "    set x = a + MyFunc()";
        assert!(!is_top_level_call_text(source, "MyFunc"), "a + NAME() should NOT be top-level");
    }

    #[test]
    fn inline_nested_as_argument() {
        let source = "    call Foo(MyFunc())";
        assert!(!is_top_level_call_text(source, "MyFunc"), "Foo(NAME()) should NOT be top-level");
    }

    #[test]
    fn inline_nested_in_comparison() {
        let source = "    if MyFunc() == true then";
        assert!(!is_top_level_call_text(source, "MyFunc"), "NAME() == true should NOT be top-level");
    }

    #[test]
    fn inline_replace_top_level_no_parens() {
        // Compound expression inlined at top-level — no parens.
        let source = "    return MyFunc()\n";
        let result = inline_call_in_source_text(source, "MyFunc", "a + b", true);
        assert_eq!(result, "    return a + b\n");
    }

    #[test]
    fn inline_replace_nested_compound_gets_parens() {
        // Compound expression inlined in a larger expression — gets parens.
        let source = "    set x = y + MyFunc()\n";
        let result = inline_call_in_source_text(source, "MyFunc", "a == b", true);
        assert_eq!(result, "    set x = y + (a == b)\n");
    }

    #[test]
    fn inline_replace_nested_simple_no_parens() {
        // Simple expression inlined in a larger expression — no parens needed.
        let source = "    set x = y + MyFunc()\n";
        let result = inline_call_in_source_text(source, "MyFunc", "42", false);
        assert_eq!(result, "    set x = y + 42\n");
    }

    #[test]
    fn inline_replace_if_condition_compound() {
        // Compound expression inlined as sole if-condition — no parens (top-level).
        let source = "    if MyFunc() then\n";
        let result = inline_call_in_source_text(source, "MyFunc", "GetUnitUserData(udg_Target) == 74", true);
        assert_eq!(result, "    if GetUnitUserData(udg_Target) == 74 then\n");
    }

    #[test]
    fn inline_replace_if_condition_nested() {
        // Compound expression inlined inside `and` — gets parens.
        let source = "    if a and MyFunc() then\n";
        let result = inline_call_in_source_text(source, "MyFunc", "GetUnitUserData(udg_Target) == 74", true);
        assert_eq!(result, "    if a and (GetUnitUserData(udg_Target) == 74) then\n");
    }

    #[test]
    fn inline_word_boundary_no_false_match() {
        // Longer name containing `MyFunc` should NOT match.
        let source = "    call MyFuncExtra()\n";
        let result = inline_call_in_source_text(source, "MyFunc", "true", false);
        assert_eq!(result, "    call MyFuncExtra()\n", "should not replace partial match");
    }

    #[test]
    fn inline_replace_call_stmt_with_call_expr() {
        // `call NAME()` where the return expression is itself a call.
        let source = "    call MyFunc()\n";
        let result = inline_call_in_source_text(source, "MyFunc", "DoSomething(x)", false);
        assert_eq!(result, "    call DoSomething(x)\n");
    }

    // ─── JASS → AS text pipeline ────────────────────────────────────────────────

    #[test]
    fn jass_to_as_precedence_false_and_true_or_true() {
        // JASS: `false and true or true` = `false and (true or true)` = false
        // AS without fix: `false and true or true` = `(false and true) or true` = true  ← WRONG
        // AS with fix: `false and (true or true)` = `false and true` = false  ← CORRECT
        let src = "\
function A takes nothing returns nothing
    local boolean x = false and true or true
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("(true or true)"),
            "expected parenthesised `(true or true)` in AS output, got:\n{result}"
        );
    }

    #[test]
    fn jass_to_as_array_becomes_table() {
        // JASS `integer array arr` → AS `table arr = {};`
        let src = "\
function A takes nothing returns nothing
    local integer array arr
endfunction
";
        let result = build_single_function_as(src);
        let expected = "table arr = {};";
        assert!(
            result.contains(expected),
            "expected `{expected}` in AS output, got:\n{result}"
        );
    }

    #[test]
    fn jass_to_as_global_array_becomes_table() {
        let result = jass_var_decl_to_as_text("unit array myUnits");
        let expected = "table myUnits = {};";
        assert_eq!(
            result.trim(),
            expected,
            "expected global `{expected}`, got: {result}"
        );
    }

    #[test]
    fn jass_to_as_function_signature() {
        let jass = "\
function Foo takes integer a, real b returns boolean
    return true
endfunction";
        let result = jass_function_to_as_text(jass);
        assert!(
            result.starts_with("bool Foo(int a, float b) {"),
            "expected AS signature, got:\n{result}"
        );
    }

    #[test]
    fn jass_to_as_basic_statements() {
        let jass = "\
function A takes nothing returns nothing
    local integer x = 5
    set x = 10
    call Foo(x)
    return
endfunction";
        let result = jass_function_to_as_text(jass);
        assert!(result.contains("int x = 5;"), "local → typed decl: {result}");
        assert!(result.contains("x = 10;"), "set → assignment: {result}");
        assert!(result.contains("Foo(x);"), "call → call: {result}");
        assert!(result.contains("return;"), "return: {result}");
    }

    #[test]
    fn jass_to_as_loop_and_exitwhen() {
        let jass = "\
function A takes nothing returns nothing
    loop
        exitwhen true
    endloop
endfunction";
        let result = jass_function_to_as_text(jass);
        assert!(result.contains("while (true) {"), "loop → while: {result}");
        assert!(result.contains("if (true) break;"), "exitwhen → if break: {result}");
    }

    #[test]
    fn jass_to_as_if_else() {
        let jass = "\
function A takes nothing returns nothing
    if x then
        call Foo()
    else
        call Bar()
    endif
endfunction";
        let result = jass_function_to_as_text(jass);
        assert!(result.contains("if (x) {"), "if: {result}");
        assert!(result.contains("} else {"), "else: {result}");
    }

    // ─── null → nil for handle types ──────────────────────────────────────────

    #[test]
    fn as_null_to_nil_local_handle() {
        // `local unit u = null` → `unit u = nil;`
        let src = "\
function A takes nothing returns nothing
    local unit u = null
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("unit u = nil;"),
            "expected `unit u = nil;` in AS output, got:\n{result}"
        );
    }

    #[test]
    fn as_null_to_nil_set_handle() {
        // `set u = null` where u is a handle → `u = nil;`
        let src = "\
function A takes nothing returns nothing
    local unit u = null
    set u = null
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("u = nil;"),
            "expected `u = nil;` in AS output, got:\n{result}"
        );
    }

    #[test]
    fn as_null_to_nil_return_handle() {
        // `return null` in a function returning a handle → `return nil;`
        let src = "\
function A takes nothing returns unit
    return null
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("return nil;"),
            "expected `return nil;` in AS output, got:\n{result}"
        );
    }

    #[test]
    fn as_null_stays_for_primitives() {
        // `local string s = null` should keep `null` (string is not a handle type).
        let src = "\
function A takes nothing returns nothing
    local string s = null
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("string s = null;"),
            "expected `string s = null;` (string is not handle), got:\n{result}"
        );
    }

    #[test]
    fn as_null_to_nil_comparison_eq() {
        // `if u == null then` → `if (u == nil) {` when u is handle
        let src = "\
function A takes nothing returns nothing
    local unit u = null
    if u == null then
        return
    endif
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("u == nil"),
            "expected `u == nil` in AS comparison, got:\n{result}"
        );
    }

    #[test]
    fn as_null_to_nil_comparison_neq() {
        // `if u != null then` → `if (u != nil) {`
        let src = "\
function A takes nothing returns nothing
    local trigger t = null
    if t != null then
        return
    endif
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("t != nil"),
            "expected `t != nil` in AS comparison, got:\n{result}"
        );
    }

    #[test]
    fn as_null_to_nil_hoisted_default() {
        // Hoisted handle-type variable should use `nil` as default.
        let src = "\
function A takes nothing returns nothing
    local integer x = 1
    set x = 2
    local unit u = null
endfunction
";
        let result = build_single_function_as(src);
        // The hoisted declaration should have nil as default.
        assert!(
            result.contains("unit u = nil;"),
            "expected hoisted `unit u = nil;` in AS output, got:\n{result}"
        );
    }

    #[test]
    fn as_null_to_nil_in_parens() {
        // `(null)` in a handle context → `(nil)`
        let src = "\
function A takes nothing returns nothing
    local unit u = (null)
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("(nil)"),
            "expected `(nil)` in AS output, got:\n{result}"
        );
    }

    #[test]
    fn as_null_stays_for_code_local() {
        // `local code c = null` → `funcdef c = null;`  (code does NOT inherit handle)
        let src = "\
function A takes nothing returns nothing
    local code c = null
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("funcdef c = null;"),
            "expected `funcdef c = null;` (code is not handle), got:\n{result}"
        );
    }

    #[test]
    fn as_null_stays_for_code_return() {
        // `return null` in a function returning code → stays `null`
        let src = "\
function A takes nothing returns code
    return null
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("return null;"),
            "expected `return null;` (code is not handle), got:\n{result}"
        );
    }

    #[test]
    fn as_null_to_nil_call_args_handle_vs_code() {
        // Self-call with mixed handle/code params:
        // 1st arg (unit) → nil, 2nd arg (code) → null
        let src = "\
function A takes unit u, code c returns nothing
    call A(null, null)
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("A(nil, null)"),
            "expected `A(nil, null)` — handle arg → nil, code arg → null, got:\n{result}"
        );
    }

    #[test]
    fn as_null_to_nil_mixed_locals() {
        // Handle → nil, code → null, string → null, integer stays as-is
        let src = "\
function A takes nothing returns nothing
    local unit u = null
    local code c = null
    local string s = null
    local trigger t = null
endfunction
";
        let result = build_single_function_as(src);
        assert!(result.contains("unit u = nil;"), "unit → nil: {result}");
        assert!(result.contains("funcdef c = null;"), "code → null: {result}");
        assert!(result.contains("string s = null;"), "string → null: {result}");
        assert!(result.contains("trigger t = nil;"), "trigger → nil: {result}");
    }

    // ─── function NAME → @NAME (funcref) ─────────────────────────────────────

    #[test]
    fn as_funcref_in_call() {
        // `call ForGroup(g, function Foo)` → `ForGroup(g, @Foo);`
        let src = "\
function A takes nothing returns nothing
    call ForGroup(g, function Foo)
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("@Foo"),
            "expected `@Foo` in AS output, got:\n{result}"
        );
        assert!(
            !result.contains("function Foo"),
            "should NOT contain `function Foo` in AS output, got:\n{result}"
        );
    }

    #[test]
    fn as_funcref_in_set() {
        // `set c = function Callback` → `c = @Callback;`
        let src = "\
function A takes nothing returns nothing
    local code c = null
    set c = function Callback
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("@Callback"),
            "expected `@Callback` in AS output, got:\n{result}"
        );
    }

    #[test]
    fn as_funcref_in_return() {
        // `return function Callback` → `return @Callback;`
        let src = "\
function A takes nothing returns code
    return function Callback
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("return @Callback;"),
            "expected `return @Callback;` in AS output, got:\n{result}"
        );
    }

    #[test]
    fn as_funcref_in_local_init() {
        // `local code c = function Callback` → `funcdef c = @Callback;`
        let src = "\
function A takes nothing returns nothing
    local code c = function Callback
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("funcdef c = @Callback;"),
            "expected `funcdef c = @Callback;` in AS output, got:\n{result}"
        );
    }

    // ─── array/table read casts ───────────────────────────────────────────────

    #[test]
    fn as_array_read_gets_cast() {
        // `integer array arr` → `table arr = {};`
        // read: `arr[i]` → `int(arr[i])`
        let src = "\
function A takes nothing returns nothing
    local integer array arr
    local integer x = arr[0]
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("int(arr[0])"),
            "expected `int(arr[0])` cast in AS output, got:\n{result}"
        );
    }

    #[test]
    fn as_array_write_no_cast() {
        // Write to array should NOT have a cast on the LHS.
        let src = "\
function A takes nothing returns nothing
    local integer array arr
    set arr[0] = 5
endfunction
";
        let result = build_single_function_as(src);
        assert!(
            result.contains("arr[0] = 5;"),
            "expected `arr[0] = 5;` without cast on LHS, got:\n{result}"
        );
    }
}


