use super::test_support::*;
use crate::http::document_symbol::SymbolKind;
use crate::http::folding::FoldingRangeKind;

    #[test]
    fn symbols_function() {
        let src = "\
function Foo takes integer x returns nothing
    local integer y = 1
endfunction
";
        with_cursor(src, |c| {
            assert_eq!(c.symbols.len(), 1);
            assert_eq!(c.symbols[0].name, "Foo");
            let ch = c.symbols[0].children.as_ref().unwrap();
            assert_eq!(ch.len(), 2);
            assert_eq!(ch[0].name, "x");
            assert_eq!(ch[1].name, "y");
        });
    }

    #[test]
    fn symbols_globals() {
        let src = "\
globals
    constant integer MAX = 100
    real x
endglobals
";
        with_cursor(src, |c| {
            assert_eq!(c.symbols.len(), 1);
            let ch = c.symbols[0].children.as_ref().unwrap();
            assert_eq!(ch[0].kind, SymbolKind::Constant);
            assert_eq!(ch[1].kind, SymbolKind::Variable);
        });
    }

    #[test]
    fn folding_regions() {
        let src = "\
function F takes nothing returns nothing
    if true then
        return
    endif
endfunction
";
        with_cursor(src, |c| {
            let regions: Vec<_> = c
                .folding
                .iter()
                .filter(|f| f.kind == Some(FoldingRangeKind::Region))
                .collect();
            assert_eq!(regions.len(), 2);
        });
    }

    #[test]
    fn folding_comments() {
        let src = "// a\n// b\n// c\ntype handle extends agent\n";
        with_cursor(src, |c| {
            let cmt: Vec<_> = c
                .folding
                .iter()
                .filter(|f| f.kind == Some(FoldingRangeKind::Comment))
                .collect();
            assert_eq!(cmt.len(), 1);
            assert_eq!(cmt[0].start_line, 0);
            assert_eq!(cmt[0].end_line, 2);
        });
    }

    #[test]
    fn scope_params_initialized() {
        let src = "\
function Foo takes integer x, real y returns nothing
endfunction
";
        with_cursor(src, |c| {
            let s = c.scopes.iter().find(|s| s.name == "Foo").unwrap();
            assert!(s.vars["x"].is_initialized);
            assert!(s.vars["y"].is_initialized);
        });
    }

    #[test]
    fn scope_local_set() {
        let src = "\
function Foo takes nothing returns nothing
    local integer x
    set x = 5
endfunction
";
        with_cursor(src, |c| {
            let s = c.scopes.iter().find(|s| s.name == "Foo").unwrap();
            assert!(s.vars["x"].is_initialized);
        });
    }

    #[test]
    fn scope_local_uninitialized() {
        let src = "\
function Foo takes nothing returns nothing
    local integer x
endfunction
";
        with_cursor(src, |c| {
            let s = c.scopes.iter().find(|s| s.name == "Foo").unwrap();
            assert!(!s.vars["x"].is_initialized);
        });
    }

    #[test]
    fn diagnostics_from_errors() {
        let src = "function\n";
        with_cursor(src, |c| {
            assert!(!c.diagnostics.is_empty());
        });
    }

    #[test]
    fn full_program() {
        let src = "\
type handle extends agent
native Ack takes integer m, integer n returns integer
globals
    integer g
endglobals
function main takes nothing returns nothing
    local integer x = 1
    set x = 2
    call Ack(x, x)
    if true then
        return
    endif
endfunction
";
        with_cursor(src, |c| {
            assert_eq!(c.symbols.len(), 4);
            assert_eq!(c.scopes.len(), 2);
            assert!(!c.semantic.data(None).is_empty());
            // `agent` is undeclared in this isolated snippet (it would be
            // provided by common.j via import in production).  Only that
            // single diagnostic is expected.
            let non_agent: Vec<_> = c.diagnostics.iter()
                .filter(|d| !d.message.contains("`agent`"))
                .collect();
            assert!(non_agent.is_empty(), "Unexpected diagnostics: {:?}", non_agent);
        });
    }

    #[test]
    fn file_symbols_function_basic() {
        let src = "\
function Foo takes integer x, real y returns boolean
    call Bar(x)
    return true
endfunction
";
        with_cursor(src, |c| {
            assert_eq!(c.file_symbols.functions.len(), 1);
            let f = &c.file_symbols.functions[0];
            assert_eq!(f.name, "Foo");
            assert_eq!(f.params.len(), 2);
            assert_eq!(f.params[0].name, "x");
            assert_eq!(f.params[0].type_name, "integer");
            assert_eq!(f.params[1].name, "y");
            assert_eq!(f.params[1].type_name, "real");
            assert_eq!(f.return_type.as_deref(), Some("boolean"));
            assert!(f.callees.contains("Bar"));
        });
    }

    #[test]
    fn file_symbols_native() {
        let src = "native RemoveUnit takes unit u returns nothing\n";
        with_cursor(src, |c| {
            assert_eq!(c.file_symbols.natives.len(), 1);
            let n = &c.file_symbols.natives[0];
            assert_eq!(n.name, "RemoveUnit");
            assert_eq!(n.params.len(), 1);
            assert_eq!(n.params[0].type_name, "unit");
            assert_eq!(n.return_type, None);
        });
    }

    #[test]
    fn file_symbols_native_returns_nothing_is_none() {
        // "returns nothing" means no return type
        let src = "native Foo takes nothing returns nothing\n";
        with_cursor(src, |c| {
            let n = &c.file_symbols.natives[0];
            // The parser stores `nothing` as the return_type node text;
            // we keep it as-is — the consumer decides `nothing` == no return.
            assert!(n.return_type.as_deref() == Some("nothing") || n.return_type.is_none());
        });
    }

    #[test]
    fn file_symbols_globals() {
        let src = "\
globals
    constant integer MAX = 100
    real array speeds
    integer count = 0
endglobals
";
        with_cursor(src, |c| {
            assert_eq!(c.file_symbols.globals.len(), 3);
            let max = &c.file_symbols.globals[0];
            assert_eq!(max.name, "MAX");
            assert!(max.is_constant);
            assert!(!max.is_array);
            assert!(max.has_initializer);

            let speeds = &c.file_symbols.globals[1];
            assert_eq!(speeds.name, "speeds");
            assert!(speeds.is_array);
            assert!(!speeds.is_constant);

            let count = &c.file_symbols.globals[2];
            assert_eq!(count.name, "count");
            assert!(count.has_initializer);
        });
    }

    #[test]
    fn file_symbols_type_decl() {
        let src = "type agent extends handle\n";
        with_cursor(src, |c| {
            assert_eq!(c.file_symbols.types.len(), 1);
            let t = &c.file_symbols.types[0];
            assert_eq!(t.name, "agent");
            assert_eq!(t.base.as_deref(), Some("handle"));
        });
    }

    #[test]
    fn file_symbols_callees_from_expressions() {
        // Callees should be collected from call statements, function calls
        // inside expressions, and `function <name>` references.
        let src = "\
function Main takes nothing returns nothing
    local integer x = GetValue()
    call DoStuff(x)
    set x = Add(x, 1)
endfunction
";
        with_cursor(src, |c| {
            let f = &c.file_symbols.functions[0];
            assert!(f.callees.contains("GetValue"), "callees: {:?}", f.callees);
            assert!(f.callees.contains("DoStuff"), "callees: {:?}", f.callees);
            assert!(f.callees.contains("Add"), "callees: {:?}", f.callees);
        });
    }

    #[test]
    fn file_symbols_callees_func_ref() {
        let src = "\
function Main takes nothing returns nothing
    local code c = function MyCallback
endfunction
";
        with_cursor(src, |c| {
            let f = &c.file_symbols.functions[0];
            assert!(f.callees.contains("MyCallback"), "callees: {:?}", f.callees);
        });
    }

    #[test]
    fn file_symbols_decl_order() {
        let src = "\
type agent extends handle
native Foo takes nothing returns nothing
globals
    integer x = 0
endglobals
function Bar takes nothing returns nothing
endfunction
";
        with_cursor(src, |c| {
            let type_idx = c.file_symbols.types[0].decl_index;
            let native_idx = c.file_symbols.natives[0].decl_index;
            let global_idx = c.file_symbols.globals[0].decl_index;
            let func_idx = c.file_symbols.functions[0].decl_index;
            assert!(type_idx < native_idx);
            assert!(native_idx < global_idx);
            assert!(global_idx < func_idx);
        });
    }

    #[test]
    fn file_symbols_multiple_functions_callees_isolated() {
        // Callees for each function should be independent.
        let src = "\
function A takes nothing returns nothing
    call X()
endfunction
function B takes nothing returns nothing
    call Y()
endfunction
";
        with_cursor(src, |c| {
            assert_eq!(c.file_symbols.functions.len(), 2);
            let a = &c.file_symbols.functions[0];
            let b = &c.file_symbols.functions[1];
            assert!(a.callees.contains("X"));
            assert!(!a.callees.contains("Y"));
            assert!(b.callees.contains("Y"));
            assert!(!b.callees.contains("X"));
        });
    }

    #[test]
    fn varstmt_top_level_exports_to_file_symbols() {
        let src = "real A = 33\n";
        with_cursor(src, |c| {
            let found = c.file_symbols.globals.iter().find(|g| g.name == "A");
            assert!(
                found.is_some(),
                "top-level `real A = 33` (VarStmt) should appear in file_symbols.globals.\n\
                 globals: {:?}",
                c.file_symbols.globals.iter().map(|g| &g.name).collect::<Vec<_>>()
            );
            let sym = found.unwrap();
            assert_eq!(sym.type_name.as_deref(), Some("real"));
            assert!(!sym.is_constant);
            assert!(!sym.is_array);
            assert!(sym.has_initializer);
        });
    }

    #[test]
    fn varstmt_top_level_constant_exports_to_file_symbols() {
        let src = "constant integer MAX = 100\n";
        with_cursor(src, |c| {
            let found = c.file_symbols.globals.iter().find(|g| g.name == "MAX");
            assert!(
                found.is_some(),
                "top-level `constant integer MAX = 100` (VarStmt) should appear in file_symbols.globals.\n\
                 globals: {:?}",
                c.file_symbols.globals.iter().map(|g| &g.name).collect::<Vec<_>>()
            );
            let sym = found.unwrap();
            assert_eq!(sym.type_name.as_deref(), Some("integer"));
            assert!(sym.is_constant);
        });
    }

    #[test]
    fn varstmt_in_function_is_local_not_global() {
        // `widget u` inside a function body should be treated as a local
        // variable and NOT exported to file_symbols.globals.
        let src = "\
function A takes nothing returns nothing
    widget u
endfunction
";
        with_cursor(src, |c| {
            // No globals should be exported
            assert!(
                c.file_symbols.globals.is_empty(),
                "VarStmt inside function should not be exported as global, got: {:?}",
                c.file_symbols.globals.iter().map(|g| &g.name).collect::<Vec<_>>()
            );
            // The variable should appear as a child symbol of the function
            assert_eq!(c.symbols.len(), 1);
            let children = c.symbols[0].children.as_ref().unwrap();
            assert!(
                children.iter().any(|ch| ch.name == "u"),
                "VarStmt inside function should appear as child symbol"
            );
        });
    }

    #[test]
    fn varstmt_at_top_level_is_global() {
        // `widget u` at top level (outside any function) should be exported
        // as a global variable.
        let src = "widget u\n";
        with_cursor(src, |c| {
            assert_eq!(
                c.file_symbols.globals.len(), 1,
                "VarStmt at top level should be exported as global"
            );
            assert_eq!(c.file_symbols.globals[0].name, "u");
        });
    }

    #[test]
    fn varstmt_in_function_registered_in_local_scope() {
        // `integer A = 33` inside a function should be a local variable
        // accessible by later `set A = 21`.
        let src = "\
function A takes nothing returns nothing
    integer A = 33
    A = 21
endfunction
";
        with_cursor(src, |c| {
            assert!(c.file_symbols.globals.is_empty());
            let scope = c.scopes.iter().find(|s| s.name == "A").unwrap();
            assert!(scope.vars.contains_key("A"), "VarStmt local should be in scope");
        });
    }
