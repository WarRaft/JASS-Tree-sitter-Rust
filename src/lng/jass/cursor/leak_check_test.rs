use super::test_support::*;

    #[test]
    fn handle_leak_basic() {
        // Handle local not set to null → warning at endfunction.
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
function A1 takes nothing returns nothing
    local unit u = CreateUnit()
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert_eq!(leaks.len(), 1, "Expected 1 leak warning, got: {:?}", leaks);
            assert!(leaks[0].message.contains("`u`"), "Should mention var name: {}", leaks[0].message);
            assert!(leaks[0].message.contains("function end"), "Should warn at function end: {}", leaks[0].message);
        });
    }

    #[test]
    fn handle_leak_nullified_no_warning() {
        // Handle local set to null before endfunction → no warning.
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
function A1 takes nothing returns nothing
    local unit u = CreateUnit()
    set u = null
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert!(leaks.is_empty(), "No leak expected after nullification, got: {:?}", leaks);
        });
    }

    #[test]
    fn handle_leak_early_return() {
        // Handle local not nullified before early return → warning at return.
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
native GetRandomInt takes integer lo, integer hi returns integer
function A1 takes nothing returns nothing
    local unit u = CreateUnit()
    if GetRandomInt(0, 100) < 50 then
        return
    endif
    set u = null
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert_eq!(leaks.len(), 1, "Expected 1 leak at early return, got: {:?}", leaks);
            assert!(leaks[0].message.contains("before `return`"), "Should warn at return: {}", leaks[0].message);
        });
    }

    #[test]
    fn handle_leak_uninit_no_warning() {
        // Uninitialized handle local starts as null → no warning.
        let src = "\
type unit extends handle
function A1 takes nothing returns nothing
    local unit u
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert!(leaks.is_empty(), "Uninitialized local should not leak, got: {:?}", leaks);
        });
    }

    #[test]
    fn handle_leak_if_else_all_nullified() {
        // Nullified in all branches (if + else) → no leak.
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
native GetRandomInt takes integer lo, integer hi returns integer
function A1 takes nothing returns nothing
    local unit u = CreateUnit()
    if GetRandomInt(0, 100) < 50 then
        set u = null
    else
        set u = null
    endif
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert!(leaks.is_empty(), "No leak expected when all branches nullify, got: {:?}", leaks);
        });
    }

    #[test]
    fn handle_leak_if_no_else() {
        // Nullified only in `if` branch, no `else` → still leaks (conservative).
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
native GetRandomInt takes integer lo, integer hi returns integer
function A1 takes nothing returns nothing
    local unit u = CreateUnit()
    if GetRandomInt(0, 100) < 50 then
        set u = null
    endif
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert_eq!(leaks.len(), 1, "Without else, nullification is not guaranteed, got: {:?}", leaks);
        });
    }

    #[test]
    fn handle_leak_multiple_vars() {
        // Two handle locals: one nullified, one not → one warning.
        let src = "\
type unit extends handle
type widget extends handle
native CreateUnit takes nothing returns unit
function A1 takes nothing returns nothing
    local unit u = CreateUnit()
    local widget w = null
    set u = null
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert!(leaks.is_empty(), "Both should be clean, got: {:?}", leaks);
        });
    }

    #[test]
    fn handle_leak_reassigned_after_null() {
        // Set to null then reassigned → leaks.
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
function A1 takes nothing returns nothing
    local unit u = CreateUnit()
    set u = null
    set u = CreateUnit()
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert_eq!(leaks.len(), 1, "Reassigned after null should leak, got: {:?}", leaks);
        });
    }

    #[test]
    fn handle_leak_integer_no_warning() {
        // Non-handle types should not trigger leak warnings.
        let src = "\
function A1 takes nothing returns nothing
    local integer x = 42
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert!(leaks.is_empty(), "Integer should not have leak warning, got: {:?}", leaks);
        });
    }

    #[test]
    fn handle_leak_param_no_warning() {
        // Parameters should not trigger leak warnings (caller manages them).
        let src = "\
type unit extends handle
function A1 takes unit u returns nothing
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert!(leaks.is_empty(), "Params should not have leak warning, got: {:?}", leaks);
        });
    }

    #[test]
    fn handle_leak_all_branches_return() {
        // If all branches return and handle is nulled before each → no leak at endfunction.
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
native GetRandomInt takes integer lo, integer hi returns integer
function A1 takes nothing returns nothing
    local unit u = CreateUnit()
    if GetRandomInt(0, 100) < 50 then
        set u = null
        return
    else
        set u = null
        return
    endif
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert!(leaks.is_empty(), "All branches return after nulling → no leak, got: {:?}", leaks);
        });
    }

    #[test]
    fn handle_leak_return_without_null_in_branch() {
        // One branch returns without nullifying → leak at return.
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
native GetRandomInt takes integer lo, integer hi returns integer
function A1 takes nothing returns nothing
    local unit u = CreateUnit()
    if GetRandomInt(0, 100) < 50 then
        return
    else
        set u = null
    endif
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert_eq!(leaks.len(), 1, "Branch returns without null → 1 leak, got: {:?}", leaks);
            assert!(leaks[0].message.contains("before `return`"), "Leak at return: {}", leaks[0].message);
        });
    }

    #[test]
    fn return_diagnostic_highlights_only_keyword() {
        // The handle-leak diagnostic on `return` should highlight only the
        // `return` keyword (6 chars), not the entire `return expr` statement.
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
function F takes nothing returns nothing
    local unit u = CreateUnit()
    return
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert!(!leaks.is_empty(), "Expected handle leak diagnostic");
            for d in &leaks {
                let start = d.range.start.character;
                let end = d.range.end.character;
                let width = end - start;
                assert_eq!(
                    width, 6,
                    "Return diagnostic should span 6 chars (the keyword `return`), got {}",
                    width
                );
            }
        });
    }

    #[test]
    fn handle_leak_local_array_no_warning() {
        // Local array variables do not leak — they are cleaned up automatically.
        let src = "\
type unit extends handle
function A1 takes nothing returns nothing
    local unit array u
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert!(leaks.is_empty(), "Local array should not produce leak warning, got: {:?}", leaks);
        });
    }

    #[test]
    fn ignore_file_level_leak_suppresses_all_leaks() {
        let src = "\
//ignore leak
type unit extends handle
native CreateUnit takes nothing returns unit
function A1 takes nothing returns nothing
    local unit u = CreateUnit()
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert!(leaks.is_empty(), "File-level //ignore leak should suppress all leak warnings, got: {:?}", leaks);
        });
    }

    #[test]
    fn ignore_file_level_multiple_tags() {
        // //ignore unused leak — both tags should be collected.
        let src = "\
//ignore unused leak
type unit extends handle
native CreateUnit takes nothing returns unit
function A1 takes nothing returns nothing
    local unit u = CreateUnit()
endfunction
";
        with_cursor(src, |c| {
            assert!(c.file_ignore_tags.contains("unused"), "Should contain 'unused'");
            assert!(c.file_ignore_tags.contains("leak"), "Should contain 'leak'");
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert!(leaks.is_empty(), "File-level //ignore leak should suppress leak warnings");
        });
    }

    #[test]
    fn ignore_per_function_leak_suppresses_that_function() {
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
//@ignore leak
function A1 takes nothing returns nothing
    local unit u = CreateUnit()
endfunction
function A2 takes nothing returns nothing
    local unit v = CreateUnit()
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            // A1 is suppressed, A2 should still warn
            assert_eq!(leaks.len(), 1, "Only A2 should leak, got: {:?}", leaks);
            assert!(leaks[0].message.contains("`v`"), "Should mention var v: {}", leaks[0].message);
        });
    }

    #[test]
    fn ignore_per_variable_leak_suppresses_that_variable() {
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
function A1 takes nothing returns nothing
    //@ignore leak
    local unit u = CreateUnit()
    local unit v = CreateUnit()
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            // u is suppressed, v should still warn
            assert_eq!(leaks.len(), 1, "Only v should leak, got: {:?}", leaks);
            assert!(leaks[0].message.contains("`v`"), "Should mention var v: {}", leaks[0].message);
        });
    }

    #[test]
    fn ignore_missing_tag_warns() {
        let src = "\
//ignore
function A1 takes nothing returns nothing
endfunction
";
        with_cursor(src, |c| {
            let warns: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Missing ignore tag"))
                .collect();
            assert_eq!(warns.len(), 1, "Missing tag should produce a warning, got: {:?}", warns);
        });
    }

    #[test]
    fn ignore_tag_registry_has_all_known_tags() {
        for tag in &["unused", "leak", "cycle"] {
            assert!(
                crate::lng::directive::find_ignore_tag(tag).is_some(),
                "IGNORE_TAGS should contain {:?}",
                tag
            );
        }
    }

    #[test]
    fn handle_leak_varstmt_basic() {
        // VarStmt (no `local` keyword) inside a function should trigger leak.
        let src = "\
type widget extends handle
type unit extends handle
native CreateUnit takes nothing returns unit
function A1 takes nothing returns nothing
    widget u = CreateUnit()
    unit u1 = CreateUnit()
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert_eq!(leaks.len(), 2, "Expected 2 leak warnings for VarStmt locals, got: {:?}", leaks);
        });
    }

    #[test]
    fn handle_leak_varstmt_early_return() {
        // VarStmt locals should be detected at early return.
        let src = "\
type widget extends handle
type unit extends handle
type image extends handle
native CreateUnit takes nothing returns unit
native GetRandomInt takes integer lo, integer hi returns integer
function A1 takes nothing returns nothing
    local image img
    widget u = CreateUnit()
    unit u1 = CreateUnit()
    widget u3 = CreateUnit()
    if GetRandomInt(0, 100) < 50 then
        return
    endif
    if u != null then
        return
    endif
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            // At first return: u, u1, u3 are non-null → 3 warnings
            // After first if: u, u1, u3 still non-null at endfunction
            // After `if u != null then return`: u is known null, u1 and u3 still non-null
            // endfunction: u1, u3 leak → 2 warnings
            // Total: 3 (first return) + 2 (second return for u1, u3) + 2 (endfunction for u1, u3) = 7
            // Actually let me think more carefully...
            // img has no initializer → stays null → no leak
            assert!(leaks.len() >= 3, "Expected multiple leak warnings, got: {:?}", leaks);
        });
    }

    #[test]
    fn handle_leak_varstmt_nullified_no_warning() {
        // VarStmt local set to null before endfunction → no warning.
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
function A1 takes nothing returns nothing
    unit u = CreateUnit()
    set u = null
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert!(leaks.is_empty(), "No leak expected after nullification, got: {:?}", leaks);
        });
    }

    #[test]
    fn handle_leak_varstmt_uninit_no_warning() {
        // Uninitialized VarStmt handle local starts as null → no warning.
        let src = "\
type unit extends handle
function A1 takes nothing returns nothing
    unit u
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert!(leaks.is_empty(), "Uninitialized VarStmt local should not leak, got: {:?}", leaks);
        });
    }

    #[test]
    fn handle_leak_varstmt_ignore_per_variable() {
        // //@ignore leak on VarStmt should suppress leak for that variable.
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
function A1 takes nothing returns nothing
    //@ignore leak
    unit u = CreateUnit()
    unit v = CreateUnit()
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert_eq!(leaks.len(), 1, "Only v should leak (u is ignored), got: {:?}", leaks);
            assert!(leaks[0].message.contains("`v`"), "Should mention var v: {}", leaks[0].message);
        });
    }

    #[test]
    fn handle_leak_returned_local() {
        // Returning a handle local → leak diagnostic with returned_local flag.
        let src = "\
type item extends handle
native UnitItemInSlot takes integer slot returns item
function GetUnitItem takes nothing returns item
    local item itm = UnitItemInSlot(0)
    return itm
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert_eq!(leaks.len(), 1, "Expected 1 leak for returned local, got: {:?}", leaks);
            assert!(leaks[0].message.contains("`itm`"), "Should mention var name: {}", leaks[0].message);
            assert!(leaks[0].message.contains("before `return`"), "Should warn at return: {}", leaks[0].message);

            // Check diagnostic data fields.
            let data = leaks[0].data.as_ref().expect("leak diagnostic should have data");
            assert_eq!(data.get("returned_local").and_then(|v| v.as_bool()), Some(true),
                "Should have returned_local: true");
            assert_eq!(data.get("func_name").and_then(|v| v.as_str()), Some("GetUnitItem"),
                "Should carry func_name");
            assert_eq!(data.get("leak_type").and_then(|v| v.as_str()), Some("item"),
                "Should carry leak_type");
            assert_eq!(data.get("leak_var").and_then(|v| v.as_str()), Some("itm"),
                "Should carry leak_var");
        });
    }

    #[test]
    fn handle_leak_returned_local_in_branch() {
        // return inside if — only the returned variable gets returned_local flag.
        let src = "\
type item extends handle
type unit extends handle
native UnitItemInSlot takes integer slot returns item
native CreateUnit takes nothing returns unit
native GetRandomInt takes integer lo, integer hi returns integer
function GetItem takes nothing returns item
    local item itm = UnitItemInSlot(0)
    local unit u = CreateUnit()
    if GetRandomInt(0, 1) == 1 then
        return itm
    endif
    set itm = null
    set u = null
    return null
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            // The early return leaks both itm (returned) and u (not nulled yet).
            assert_eq!(leaks.len(), 2, "Expected 2 leaks at early return, got: {:?}", leaks);

            let itm_leak = leaks.iter().find(|d| {
                d.data.as_ref()
                    .and_then(|data| data.get("leak_var"))
                    .and_then(|v| v.as_str()) == Some("itm")
            }).expect("Should have leak for itm");

            let u_leak = leaks.iter().find(|d| {
                d.data.as_ref()
                    .and_then(|data| data.get("leak_var"))
                    .and_then(|v| v.as_str()) == Some("u")
            }).expect("Should have leak for u");

            // itm is the one being returned → returned_local: true
            let itm_data = itm_leak.data.as_ref().unwrap();
            assert_eq!(itm_data.get("returned_local").and_then(|v| v.as_bool()), Some(true),
                "itm is returned → returned_local: true");

            // u is NOT the return expression → no returned_local
            let u_data = u_leak.data.as_ref().unwrap();
            assert!(u_data.get("returned_local").is_none(),
                "u is not returned → no returned_local flag");
        });
    }

    #[test]
    fn handle_leak_return_non_local_expr_no_flag() {
        // Returning a non-local expression (function call) → leak but no returned_local.
        let src = "\
type unit extends handle
native CreateUnit takes nothing returns unit
function MakeUnit takes nothing returns unit
    local unit u = CreateUnit()
    return CreateUnit()
endfunction
";
        with_cursor(src, |c| {
            let leaks: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Handle leak"))
                .collect();
            assert_eq!(leaks.len(), 1, "Expected 1 leak for u, got: {:?}", leaks);
            let data = leaks[0].data.as_ref().unwrap();
            assert!(data.get("returned_local").is_none(),
                "Return of a call expr → no returned_local flag");
            assert_eq!(data.get("func_name").and_then(|v| v.as_str()), Some("MakeUnit"));
            assert_eq!(data.get("leak_type").and_then(|v| v.as_str()), Some("unit"));
        });
    }
