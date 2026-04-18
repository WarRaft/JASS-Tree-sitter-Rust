use super::test_support::*;
use crate::lng::jass::cursor::{ImportedKind, ImportedSymbol};
use url::Url;

    #[test]
    fn type_map_type_decl() {
        use crate::lng::jass::type_map::DeclType;
        let src = "type widget extends agent\n";
        with_cursor(src, |c| {
            let type_entries: Vec<_> = c.type_map.entries.values()
                .filter(|d| matches!(d, DeclType::Type(_)))
                .collect();
            assert_eq!(type_entries.len(), 1, "should have one type entry");
            if let DeclType::Type(info) = type_entries[0] {
                assert_eq!(info.base.as_deref(), Some("agent"));
            } else {
                panic!("expected DeclType::Type");
            }
        });
    }

    #[test]
    fn type_map_native() {
        use crate::lng::jass::type_map::DeclType;
        let src = "native CreateUnit takes player p, integer id returns unit\n";
        with_cursor(src, |c| {
            let func_entries: Vec<_> = c.type_map.entries.values()
                .filter(|d| matches!(d, DeclType::Func(_)))
                .collect();
            assert_eq!(func_entries.len(), 1, "should have one func entry for native");
            if let DeclType::Func(ft) = func_entries[0] {
                assert_eq!(ft.params.len(), 2);
                assert_eq!(ft.params[0].name, "p");
                assert_eq!(ft.params[0].type_name, "player");
                assert_eq!(ft.params[1].name, "id");
                assert_eq!(ft.params[1].type_name, "integer");
                assert_eq!(ft.return_type.as_deref(), Some("unit"));
            } else {
                panic!("expected DeclType::Func");
            }
        });
    }

    #[test]
    fn type_map_function_with_params() {
        use crate::lng::jass::type_map::DeclType;
        let src = "\
function Foo takes integer x, real y returns nothing
endfunction
";
        with_cursor(src, |c| {
            // Function itself
            let func_entries: Vec<_> = c.type_map.entries.values()
                .filter(|d| matches!(d, DeclType::Func(_)))
                .collect();
            assert_eq!(func_entries.len(), 1, "should have func entry for Foo");

            // Parameters
            let var_entries: Vec<_> = c.type_map.entries.values()
                .filter(|d| matches!(d, DeclType::Var(_)))
                .collect();
            assert_eq!(var_entries.len(), 2, "should have var entries for x and y");
            let types: Vec<&str> = var_entries.iter().map(|d| {
                if let DeclType::Var(vt) = d { vt.name.as_str() } else { "" }
            }).collect();
            assert!(types.contains(&"integer"));
            assert!(types.contains(&"real"));
        });
    }

    #[test]
    fn type_map_globals() {
        use crate::lng::jass::type_map::DeclType;
        let src = "\
globals
    constant integer MAX = 100
    real x = 1.5
endglobals
";
        with_cursor(src, |c| {
            let var_entries: Vec<_> = c.type_map.entries.iter()
                .filter(|(_, d)| matches!(d, DeclType::Var(_)))
                .collect();
            assert_eq!(var_entries.len(), 2, "should have var entries for MAX and x");

            // Find the constant
            let constant = var_entries.iter()
                .find(|(_, d)| matches!(d, DeclType::Var(vt) if vt.is_constant))
                .map(|(_, d)| d);
            assert!(constant.is_some(), "MAX should be constant");
            if let Some(DeclType::Var(vt)) = constant {
                assert_eq!(vt.name, "integer");
                assert!(vt.is_constant);
                assert!(vt.is_comptime, "constant with literal init should be comptime");
            }
        });
    }

    #[test]
    fn type_map_local() {
        use crate::lng::jass::type_map::DeclType;
        let src = "\
function Foo takes nothing returns nothing
    local integer x = 5
endfunction
";
        with_cursor(src, |c| {
            let local_entries: Vec<_> = c.type_map.entries.values()
                .filter(|d| matches!(d, DeclType::Var(vt) if vt.name == "integer" && !vt.is_constant))
                .collect();
            assert_eq!(local_entries.len(), 1, "should have local var entry for x");
        });
    }

    #[test]
    fn type_hints_generated_for_globals() {
        let src = "\
globals
    integer x = 5
endglobals
";
        with_cursor(src, |c| {
            assert!(!c.type_hints.is_empty(), "should have type hints for globals");
            assert!(
                c.type_hints.iter().any(|h| h.label.contains("integer")),
                "type hint should mention 'integer'"
            );
        });
    }

    #[test]
    fn type_hints_generated_for_locals() {
        let src = "\
function Foo takes nothing returns nothing
    local real y = 3.14
endfunction
";
        with_cursor(src, |c| {
            assert!(!c.type_hints.is_empty(), "should have type hints for locals");
            assert!(
                c.type_hints.iter().any(|h| h.label.contains("real")),
                "type hint should mention 'real'"
            );
        });
    }

    #[test]
    fn type_hints_generated_for_varstmt_toplevel() {
        let src = "real A = 33\n";
        with_cursor(src, |c| {
            assert!(!c.type_hints.is_empty(), "should have type hints for top-level VarStmt");
            assert!(
                c.type_hints.iter().any(|h| h.label.contains("real")),
                "type hint should mention 'real', hints: {:?}",
                c.type_hints.iter().map(|h| &h.label).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn type_hints_generated_for_varstmt_in_function() {
        let src = "\
function A takes nothing returns nothing
    integer x = 33
    unit u = null
endfunction
";
        with_cursor(src, |c| {
            let labels: Vec<&str> = c.type_hints.iter().map(|h| h.label.as_str()).collect();
            assert!(
                labels.iter().any(|l| l.contains("integer")),
                "should have type hint for integer, got: {:?}", labels
            );
            assert!(
                labels.iter().any(|l| l.contains("unit")),
                "should have type hint for unit, got: {:?}", labels
            );
        });
    }

    #[test]
    fn comptime_propagation() {
        use crate::lng::jass::type_map::DeclType;
        let src = "\
globals
    constant integer A = 10
    constant integer B = A + 5
    constant integer C = A * B
endglobals
";
        with_cursor(src, |c| {
            let comptime_count = c.type_map.entries.values()
                .filter(|d| matches!(d, DeclType::Var(vt) if vt.is_comptime))
                .count();
            assert_eq!(comptime_count, 3, "all three constants with comptime inits should be comptime");
        });
    }

    #[test]
    fn type_hint_integer_literal() {
        let src = "\
globals
    integer x = 5
endglobals
";
        with_cursor(src, |c| {
            assert!(
                c.type_hints.iter().any(|h| h.label == ": integer" && h.position.line == 1),
                "should have ': integer' hint for literal 5, got: {:?}",
                c.type_hints.iter().map(|h| (&h.label, h.position.line, h.position.character)).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn type_hint_real_literal() {
        let src = "\
globals
    real y = 3.14
endglobals
";
        with_cursor(src, |c| {
            assert!(
                c.type_hints.iter().any(|h| h.label == ": real" && h.position.line == 1
                    && h.position.character > 10),
                "should have ': real' hint for literal 3.14, got: {:?}",
                c.type_hints.iter().map(|h| (&h.label, h.position.line, h.position.character)).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn type_hint_string_literal() {
        let src = "\
globals
    string s = \"hello\"
endglobals
";
        with_cursor(src, |c| {
            assert!(
                c.type_hints.iter().any(|h| h.label == ": string" && h.position.line == 1
                    && h.position.character > 14),
                "should have ': string' hint for literal \"hello\", got: {:?}",
                c.type_hints.iter().map(|h| (&h.label, h.position.line, h.position.character)).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn type_hint_boolean_literal() {
        let src = "\
globals
    boolean b = true
endglobals
";
        with_cursor(src, |c| {
            assert!(
                c.type_hints.iter().any(|h| h.label == ": boolean" && h.position.line == 1
                    && h.position.character > 14),
                "should have ': boolean' hint for 'true', got: {:?}",
                c.type_hints.iter().map(|h| (&h.label, h.position.line, h.position.character)).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn type_hint_variable_reference() {
        let src = "\
globals
    integer a = 10
    integer b = a
endglobals
";
        with_cursor(src, |c| {
            // Line 2 is `integer b = a` — the `a` reference should get `: integer`
            let a_hints: Vec<_> = c.type_hints.iter()
                .filter(|h| h.position.line == 2 && h.label == ": integer")
                .collect();
            assert!(
                a_hints.len() >= 2,
                "line 2 should have ': integer' for both 'b' decl and 'a' reference, got: {:?}",
                c.type_hints.iter().map(|h| (&h.label, h.position.line, h.position.character)).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn type_hint_function_call() {
        let src = "\
function GetVal takes nothing returns integer
endfunction
function Foo takes nothing returns nothing
    local integer x = GetVal()
endfunction
";
        with_cursor(src, |c| {
            // The `GetVal()` call should produce a `: integer` hint
            assert!(
                c.type_hints.iter().any(|h| h.label == ": integer" && h.position.line == 3),
                "should have ': integer' hint for GetVal() call, got: {:?}",
                c.type_hints.iter().map(|h| (&h.label, h.position.line, h.position.character)).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn type_hint_func_ref_is_code() {
        let src = "\
function Foo takes nothing returns nothing
endfunction
function Bar takes nothing returns nothing
    local code c = function Foo
endfunction
";
        with_cursor(src, |c| {
            assert!(
                c.type_hints.iter().any(|h| h.label == ": code"),
                "should have ': code' hint for function reference, got: {:?}",
                c.type_hints.iter().map(|h| (&h.label, h.position.line, h.position.character)).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn type_hint_binary_arithmetic() {
        // 2 * 3.14 → each literal gets a hint; result type is real
        let src = "\
globals
    real x = 2 * 3.14
endglobals
";
        with_cursor(src, |c| {
            let labels: Vec<_> = c.type_hints.iter()
                .filter(|h| h.position.line == 1)
                .map(|h| h.label.as_str())
                .collect();
            assert!(labels.contains(&": integer"), "should hint '2' as integer, got: {:?}", labels);
            assert!(labels.contains(&": real"), "should hint '3.14' as real, got: {:?}", labels);
        });
    }

    #[test]
    fn type_hint_null_literal() {
        let src = "\
function Foo takes nothing returns nothing
    local handle h = null
endfunction
";
        with_cursor(src, |c| {
            assert!(
                c.type_hints.iter().any(|h| h.label == ": null"),
                "should have ': null' hint for null, got: {:?}",
                c.type_hints.iter().map(|h| (&h.label, h.position.line, h.position.character)).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn unknown_type_string_times_integer() {
        // `"hello" * 3` → type `unknown`
        let src = "\
globals
    integer x = \"hello\" * 3
endglobals
";
        with_cursor(src, |c| {
            // The declaration hint on `x` should show `unknown` because
            // the initialiser expression "hello" * 3 is not valid.
            let decl_hint = c.type_hints.iter()
                .find(|h| h.position.line == 1 && h.label.contains("integer"));
            assert!(
                decl_hint.is_some(),
                "should have a type hint for variable x, got: {:?}",
                c.type_hints.iter().map(|h| (&h.label, h.position.line)).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn unknown_type_boolean_minus_boolean() {
        // `false - true` → both operands are boolean, minus is invalid → unknown
        let src = "\
function Foo takes nothing returns nothing
    local integer x = false - true
endfunction
";
        with_cursor(src, |c| {
            // The expression `false - true` type should be unknown.
            // We check that `false` gets `: boolean` and `true` gets `: boolean`
            let bool_hints: Vec<_> = c.type_hints.iter()
                .filter(|h| h.label == ": boolean")
                .collect();
            assert!(bool_hints.len() >= 2, "both `false` and `true` should get `: boolean`, got: {:?}",
                c.type_hints.iter().map(|h| (&h.label, h.position.line, h.position.character)).collect::<Vec<_>>());
        });
    }

    #[test]
    fn unknown_type_negate_string() {
        // `-"hello"` → unary minus on string → unknown
        let src = "\
globals
    integer x = -\"hello\"
endglobals
";
        with_cursor(src, |c| {
            // The string literal should still get `: string`
            assert!(
                c.type_hints.iter().any(|h| h.label == ": string"),
                "string literal should get `: string`"
            );
        });
    }

    #[test]
    fn unknown_type_not_integer() {
        // `not 5` → `not` on non-boolean → unknown
        let src = "\
globals
    boolean b = not 5
endglobals
";
        with_cursor(src, |c| {
            assert!(
                c.type_hints.iter().any(|h| h.label == ": integer"),
                "literal 5 should get `: integer`"
            );
        });
    }

    #[test]
    fn comptime_value_integer_on_global() {
        let src = "\
globals
    constant integer A = 10
endglobals
";
        with_cursor(src, |c| {
            let hint = c.type_hints.iter()
                .find(|h| h.label.contains("comptime") && h.label.contains("integer"));
            assert!(
                hint.is_some(),
                "constant integer global should get comptime hint, got: {:?}",
                c.type_hints.iter().map(|h| &h.label).collect::<Vec<_>>()
            );
            let h = hint.unwrap();
            assert!(
                h.label.contains("(10)"),
                "hint should contain comptime value (10), got: {:?}", h.label
            );
        });
    }

    #[test]
    fn comptime_value_string_concat() {
        let src = "\
globals
    constant string S = \"a\" + \"b\"
endglobals
";
        with_cursor(src, |c| {
            let hint = c.type_hints.iter()
                .find(|h| h.label.contains("comptime") && h.label.contains("string"));
            assert!(
                hint.is_some(),
                "constant string concat should get comptime hint, got: {:?}",
                c.type_hints.iter().map(|h| &h.label).collect::<Vec<_>>()
            );
            let h = hint.unwrap();
            assert!(
                h.label.contains("(ab)"),
                "hint should contain comptime value (ab), got: {:?}", h.label
            );
        });
    }

    #[test]
    fn comptime_value_string_plus_integer() {
        // "a" + 1 → comptime value "a1"
        let src = "\
globals
    constant string S = \"a\" + 1
endglobals
";
        with_cursor(src, |c| {
            let hint = c.type_hints.iter()
                .find(|h| h.label.contains("string") && h.label.contains("(a1)"));
            assert!(
                hint.is_some(),
                "constant string + int should give comptime value (a1), got: {:?}",
                c.type_hints.iter().map(|h| &h.label).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn comptime_value_propagates_through_globals() {
        let src = "\
globals
    constant integer A = 10
    constant integer B = A + 5
endglobals
";
        with_cursor(src, |c| {
            // B = A + 5 = 15
            let hint = c.type_hints.iter()
                .find(|h| h.position.line == 2 && h.label.contains("(15)"));
            assert!(
                hint.is_some(),
                "B should have comptime value (15), got: {:?}",
                c.type_hints.iter()
                    .filter(|h| h.position.line == 2)
                    .map(|h| &h.label)
                    .collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn comptime_value_real_arithmetic() {
        let src = "\
globals
    constant real R = 2.5 * 4.0
endglobals
";
        with_cursor(src, |c| {
            let hint = c.type_hints.iter()
                .find(|h| h.label.contains("real") && h.label.contains("(10"));
            assert!(
                hint.is_some(),
                "constant real should have comptime value ≈10, got: {:?}",
                c.type_hints.iter().map(|h| &h.label).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn comptime_value_boolean_logic() {
        let src = "\
globals
    constant boolean B = true and false
endglobals
";
        with_cursor(src, |c| {
            let hint = c.type_hints.iter()
                .find(|h| h.label.contains("boolean") && h.label.contains("(false)"));
            assert!(
                hint.is_some(),
                "constant boolean `true and false` should give comptime value (false), got: {:?}",
                c.type_hints.iter().map(|h| &h.label).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn comptime_value_not_shown_on_non_constant() {
        // Non-constant globals should NOT show comptime value (they're mutable)
        let src = "\
globals
    integer X = 42
endglobals
";
        with_cursor(src, |c| {
            let hint = c.type_hints.iter()
                .find(|h| h.position.line == 1 && h.label.contains("integer"));
            assert!(hint.is_some());
            let h = hint.unwrap();
            assert!(
                !h.label.contains("(42)"),
                "non-constant global should NOT show comptime value, got: {:?}", h.label
            );
        });
    }

    #[test]
    fn comptime_value_local_shows_value() {
        // Locals CAN show comptime value of the initialiser
        let src = "\
function Foo takes nothing returns nothing
    local integer x = 7 + 3
endfunction
";
        with_cursor(src, |c| {
            let hint = c.type_hints.iter()
                .find(|h| h.position.line == 1 && h.label.contains("integer") && h.label.contains("(10)"));
            assert!(
                hint.is_some(),
                "local x should show comptime value (10), got: {:?}",
                c.type_hints.iter()
                    .filter(|h| h.position.line == 1)
                    .map(|h| &h.label)
                    .collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn comptime_value_varstmt_toplevel() {
        // Top-level VarStmt also shows comptime value
        let src = "constant integer MAX = 100\n";
        with_cursor(src, |c| {
            let hint = c.type_hints.iter()
                .find(|h| h.label.contains("comptime") && h.label.contains("(100)"));
            assert!(
                hint.is_some(),
                "top-level constant should show comptime value (100), got: {:?}",
                c.type_hints.iter().map(|h| &h.label).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn comptime_value_hex_literal() {
        let src = "\
globals
    constant integer H = 0xFF
endglobals
";
        with_cursor(src, |c| {
            let hint = c.type_hints.iter()
                .find(|h| h.label.contains("(255)"));
            assert!(
                hint.is_some(),
                "hex literal 0xFF should evaluate to 255, got: {:?}",
                c.type_hints.iter().map(|h| &h.label).collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn comptime_value_division_by_zero_no_value() {
        // Division by zero should not produce a comptime value
        let src = "\
globals
    constant integer D = 10 / 0
endglobals
";
        with_cursor(src, |c| {
            // Should still have a hint but without comptime value
            let hint = c.type_hints.iter()
                .find(|h| h.position.line == 1 && h.label.contains("integer"));
            assert!(hint.is_some());
            let h = hint.unwrap();
            // The label should NOT have a parenthesised value
            assert!(
                !h.label.contains("("),
                "div by zero should not produce comptime value, got: {:?}", h.label
            );
        });
    }

    #[test]
    fn undeclared_variable_diagnostic() {
        // `e` and `r` are not declared anywhere.
        let src = "boolean T = e\n";
        with_cursor(src, |c| {
            let undecl: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Undeclared"))
                .collect();
            assert!(
                undecl.iter().any(|d| d.message.contains("`e`")),
                "Expected diagnostic for undeclared `e`, got: {:?}", undecl
            );
        });
    }

    #[test]
    fn undeclared_variables_in_expression() {
        // `e` and `r` are not declared — both should get diagnostics.
        let src = "\
boolean A = true
boolean T = A and e or r
";
        with_cursor(src, |c| {
            let undecl: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Undeclared"))
                .collect();
            assert!(
                undecl.iter().any(|d| d.message.contains("`e`")),
                "Expected diagnostic for undeclared `e`, got: {:?}", undecl
            );
            assert!(
                undecl.iter().any(|d| d.message.contains("`r`")),
                "Expected diagnostic for undeclared `r`, got: {:?}", undecl
            );
        });
    }

    #[test]
    fn unknown_propagates_through_binary() {
        // `e` and `r` are undeclared → type `unknown`.
        // Diagnostics on operators: `and` (boolean × unknown), `or` (unknown × unknown).
        // No "Cannot assign" on `=` — `unknown` is tolerated so that the
        // more precise operator / undeclared diagnostics are not doubled.
        // Phase 2: "Undeclared" for `e` and `r`.
        let src = "\
boolean A = true
boolean T = A and e or r
";
        with_cursor(src, |c| {
            let op_err: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Operator"))
                .collect();
            assert!(
                !op_err.is_empty(),
                "Expected operator diagnostics for `and`/`or`, got: {:?}",
                c.diagnostics
            );
            let assign_err: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                assign_err.is_empty(),
                "No assignment mismatch expected for `unknown` (error already on operator), got: {:?}",
                c.diagnostics
            );
            let undecl: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Undeclared"))
                .collect();
            assert!(
                !undecl.is_empty(),
                "Expected Undeclared diagnostics for `e` and `r`, got: {:?}",
                c.diagnostics
            );
        });
    }

    #[test]
    fn declared_variable_no_undeclared_diagnostic() {
        // `A` is declared — no "Undeclared" diagnostic expected.
        let src = "\
boolean A = true
boolean T = A and true
";
        with_cursor(src, |c| {
            let undecl: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Undeclared"))
                .collect();
            assert!(
                undecl.is_empty(),
                "No undeclared diagnostics expected for fully declared code, got: {:?}", undecl
            );
        });
    }

    #[test]
    fn unknown_type_mismatch_in_local() {
        // Inside a function, `x` is undeclared → type unknown.
        // No "Cannot assign" on `=` — `unknown` is tolerated.
        // Phase 2: "Undeclared" for `x`.
        let src = "\
function F takes nothing returns nothing
    local integer y = x
endfunction
";
        with_cursor(src, |c| {
            let assign_err: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                assign_err.is_empty(),
                "No assignment mismatch expected for `unknown`, got: {:?}", c.diagnostics
            );
            let undecl: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Undeclared"))
                .collect();
            assert!(
                !undecl.is_empty(),
                "Expected Undeclared diagnostic for `x`, got: {:?}", c.diagnostics
            );
        });
    }

    #[test]
    fn unknown_propagates_through_arithmetic() {
        // `x` is undeclared → unknown.
        // `1 + x` → operator `+` error (integer × unknown).
        // No "Cannot assign" on `=` — `unknown` is tolerated.
        // Phase 2: "Undeclared" for `x`.
        let src = "integer a = 1 + x\n";
        with_cursor(src, |c| {
            let op_err: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Operator"))
                .collect();
            assert!(
                !op_err.is_empty(),
                "Expected operator diagnostic for `+`, got: {:?}", c.diagnostics
            );
            let assign_err: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                assign_err.is_empty(),
                "No assignment mismatch expected for `unknown`, got: {:?}", c.diagnostics
            );
            let undecl: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Undeclared"))
                .collect();
            assert!(
                !undecl.is_empty(),
                "Expected Undeclared diagnostic for `x`, got: {:?}", c.diagnostics
            );
        });
    }

    #[test]
    fn no_mismatch_when_all_known() {
        // All variables declared and types match.
        let src = "\
integer a = 1
integer b = a + 2
";
        with_cursor(src, |c| {
            let mismatch: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                mismatch.is_empty(),
                "No type mismatch expected, got: {:?}", mismatch
            );
        });
    }

    #[test]
    fn type_mismatch_handle_to_real() {
        // `CreateUnit` returns `unit` (handle subtype) → assigning to `real` is an error.
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
function A1 takes nothing returns nothing
    local real u = CreateUnit()
endfunction
";
        with_cursor(src, |c| {
            let mismatch: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                !mismatch.is_empty(),
                "Expected type mismatch (unit → real), got: {:?}", c.diagnostics
            );
            assert!(
                mismatch[0].message.contains("`unit`") && mismatch[0].message.contains("`real`"),
                "Message should mention both types, got: {}", mismatch[0].message
            );
        });
    }

    #[test]
    fn type_mismatch_boolean_to_integer() {
        let src = "integer a = true\n";
        with_cursor(src, |c| {
            let mismatch: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                !mismatch.is_empty(),
                "Expected type mismatch (boolean → integer), got: {:?}", c.diagnostics
            );
        });
    }

    #[test]
    fn type_mismatch_string_to_integer() {
        let src = "integer a = \"hello\"\n";
        with_cursor(src, |c| {
            let mismatch: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                !mismatch.is_empty(),
                "Expected type mismatch (string → integer), got: {:?}", c.diagnostics
            );
        });
    }

    #[test]
    fn type_mismatch_real_to_integer() {
        // real → integer is NOT allowed (no implicit R2I).
        let src = "integer a = 1.5\n";
        with_cursor(src, |c| {
            let mismatch: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                !mismatch.is_empty(),
                "Expected type mismatch (real → integer), got: {:?}", c.diagnostics
            );
        });
    }

    #[test]
    fn no_mismatch_integer_to_real() {
        // integer → real is OK (implicit I2R).
        let src = "real a = 1\n";
        with_cursor(src, |c| {
            let mismatch: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                mismatch.is_empty(),
                "No mismatch expected for integer → real, got: {:?}", mismatch
            );
        });
    }

    #[test]
    fn no_mismatch_null_to_handle() {
        // null → handle-derived type is OK.
        let src = "\
globals
    unit u = null
endglobals
";
        with_cursor(src, |c| {
            let mismatch: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                mismatch.is_empty(),
                "No mismatch expected for null → handle type, got: {:?}", mismatch
            );
        });
    }

    #[test]
    fn no_mismatch_null_to_string() {
        // null → string is OK.
        let src = "string s = null\n";
        with_cursor(src, |c| {
            let mismatch: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                mismatch.is_empty(),
                "No mismatch expected for null → string, got: {:?}", mismatch
            );
        });
    }

    #[test]
    fn type_mismatch_null_to_integer() {
        // null → integer is NOT allowed.
        let src = "integer a = null\n";
        with_cursor(src, |c| {
            let mismatch: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                !mismatch.is_empty(),
                "Expected type mismatch (null → integer), got: {:?}", c.diagnostics
            );
        });
    }

    #[test]
    fn type_mismatch_null_to_boolean() {
        // null → boolean is NOT allowed.
        let src = "boolean b = null\n";
        with_cursor(src, |c| {
            let mismatch: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                !mismatch.is_empty(),
                "Expected type mismatch (null → boolean), got: {:?}", c.diagnostics
            );
        });
    }

    #[test]
    fn no_mismatch_handle_subtypes() {
        // Both handle-derived → JASS allows implicit handle casts.
        let src = "\
native GetTriggerUnit takes nothing returns unit
globals
    handle h = GetTriggerUnit()
endglobals
";
        with_cursor(src, |c| {
            let mismatch: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                mismatch.is_empty(),
                "No mismatch expected between handle subtypes, got: {:?}", mismatch
            );
        });
    }

    #[test]
    fn type_mismatch_in_set_statement() {
        // `set` statement: assigning boolean to an integer variable.
        let src = "\
globals
    integer x = 0
endglobals
function F takes nothing returns nothing
    set x = true
endfunction
";
        with_cursor(src, |c| {
            let mismatch: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                !mismatch.is_empty(),
                "Expected type mismatch in set statement (boolean → integer), got: {:?}", c.diagnostics
            );
        });
    }

    #[test]
    fn type_mismatch_imported_func_return() {
        // Imported `CreateUnit` returns `unit` — assigning to `real` is an error.
        let origin = Url::parse("file:///common.j").unwrap();
        let imported = vec![
            ImportedSymbol {
                origin_uri: origin.clone(),
                name: "CreateUnit".into(),
                kind: ImportedKind::Func,
                origin_decl_key: Some(0),
                return_type: Some("unit".into()),
                type_name: None,
            },
        ];
        let src = "\
function A1 takes nothing returns nothing
    local real u = CreateUnit()
endfunction
";
        with_cursor_imported(src, &imported, |c| {
            let mismatch: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Cannot assign"))
                .collect();
            assert!(
                !mismatch.is_empty(),
                "Expected type mismatch (unit → real) from imported function, got: {:?}", c.diagnostics
            );
        });
    }
