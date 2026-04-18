use super::test_support::*;
use crate::http::position::Position;
use crate::http::ref_map::EXTERNAL_KEY_BASE;
use crate::lng::jass::cursor::{ImportedKind, ImportedSymbol};
use lapce_xi_rope::Rope;
use url::Url;

    #[test]
    fn highlight_local_scoped_to_function() {
        // Variable `A` is declared in globals AND as local in function.
        // Highlighting local `A` should NOT include global `A`.
        let src = "\
globals
    real A = 33
endglobals
function A takes nothing returns nothing
    local integer A = 33
    set A = 21
endfunction
";
        with_cursor(src, |c| {
            // Global `A` at line 1
            let global_a_groups: Vec<_> = c
                .ref_groups
                .iter()
                .filter(|(_, occs)| occs.iter().any(|o| o.range.start.line == 1))
                .collect();
            assert!(
                !global_a_groups.is_empty(),
                "Should have ref group for global A"
            );

            // Local `A` at line 4
            let local_a_groups: Vec<_> = c
                .ref_groups
                .iter()
                .filter(|(_, occs)| occs.iter().any(|o| o.range.start.line == 4))
                .collect();
            assert!(
                !local_a_groups.is_empty(),
                "Should have ref group for local A"
            );

            // They should be in different groups
            let global_key = global_a_groups[0].0;
            let local_key = local_a_groups[0].0;
            assert_ne!(
                global_key, local_key,
                "Global and local 'A' should be in different ref groups"
            );

            // Local group should contain the declaration (line 4) and the set (line 5)
            let local_occs = &c.ref_groups[local_key];
            let local_lines: Vec<usize> = local_occs.iter().map(|o| o.range.start.line).collect();
            assert!(
                local_lines.contains(&4),
                "Local group should contain line 4 (decl)"
            );
            assert!(
                local_lines.contains(&5),
                "Local group should contain line 5 (set)"
            );
            assert!(
                !local_lines.contains(&1),
                "Local group should NOT contain line 1 (global)"
            );

            // Global group should contain only the global declaration
            let global_occs = &c.ref_groups[global_key];
            let global_lines: Vec<usize> = global_occs.iter().map(|o| o.range.start.line).collect();
            assert!(
                global_lines.contains(&1),
                "Global group should contain line 1"
            );
            assert!(
                !global_lines.contains(&4),
                "Global group should NOT contain line 4"
            );
        });
    }

    #[test]
    fn highlight_function_name_includes_call() {
        let src = "\
function Foo takes nothing returns nothing
endfunction
function Bar takes nothing returns nothing
    call Foo()
endfunction
";
        with_cursor(src, |c| {
            let foo_groups: Vec<_> = c
                .ref_groups
                .iter()
                .filter(|(_, occs)| {
                    occs.iter()
                        .any(|o| o.range.start.line == 0 && o.range.start.character == 9)
                })
                .collect();
            assert!(!foo_groups.is_empty(), "Should have group for Foo");

            let foo_occs = &c.ref_groups[foo_groups[0].0];
            let foo_lines: Vec<usize> = foo_occs.iter().map(|o| o.range.start.line).collect();
            assert!(
                foo_lines.contains(&0),
                "Foo group should contain line 0 (decl)"
            );
            assert!(
                foo_lines.contains(&3),
                "Foo group should contain line 3 (call)"
            );
        });
    }

    #[test]
    fn definition_points_to_declaration() {
        let src = "\
globals
    integer x = 0
endglobals
function Foo takes nothing returns nothing
    set x = 1
endfunction
";
        let rope = Rope::from(src);
        with_cursor(src, |c| {
            let rm = ref_map_from(c, &rope);
            // "set x = 1" — `x` is at line 4, char 8 (after "    set ")
            let byte = Position {
                line: 4,
                character: 8,
            }
            .to_byte_offset(&rope)
            .unwrap();
            let defs = rm.definitions_at(byte);
            assert_eq!(defs.len(), 1, "Should have exactly one definition");
            assert_eq!(
                defs[0].range.start.line, 1,
                "Definition should be on line 1 (globals)"
            );
            assert!(defs[0].is_decl);
        });
    }

    #[test]
    fn references_includes_all_usages() {
        let src = "\
function Foo takes nothing returns nothing
endfunction
function Bar takes nothing returns nothing
    call Foo()
endfunction
";
        let rope = Rope::from(src);
        with_cursor(src, |c| {
            let rm = ref_map_from(c, &rope);
            // `Foo` decl at line 0, char 9
            let byte = Position {
                line: 0,
                character: 9,
            }
            .to_byte_offset(&rope)
            .unwrap();
            let all = rm.occurrences_at(byte);
            assert_eq!(all.len(), 2, "Should have 2 occurrences (decl + call)");
            let decls: Vec<_> = all.iter().filter(|o| o.is_decl).collect();
            assert_eq!(decls.len(), 1, "Should have 1 declaration");
            let refs: Vec<_> = all.iter().filter(|o| !o.is_decl).collect();
            assert_eq!(refs.len(), 1, "Should have 1 reference");
        });
    }

    #[test]
    fn ref_varstmt_local_set_call() {
        // Exact user scenario: VarStmt on top-level, function with same name,
        // local shadowing, set (without `set` keyword), and call.
        let src = "\
real A = 33
function A takes nothing returns nothing
    local integer A = 33
    set A = 21
endfunction
call A()
";
        let rope = Rope::from(src);
        with_cursor(src, |c| {
            let rm = ref_map_from(c, &rope);

            // 1. Top-level VarStmt `real A = 33` — A at line 0, char 5
            let byte_var = Position { line: 0, character: 5 }
                .to_byte_offset(&rope).unwrap();
            let var_occs = rm.occurrences_at(byte_var);
            assert!(!var_occs.is_empty(),
                "VarStmt 'A' at line 0 should have ref group, byte={}. groups: {:?}",
                byte_var, c.ref_names);
            // It's a declaration
            assert!(var_occs.iter().any(|o| o.is_decl),
                "VarStmt 'A' should be a declaration");

            // 2. Function name `A` at line 1, char 9
            let byte_func = Position { line: 1, character: 9 }
                .to_byte_offset(&rope).unwrap();
            let func_occs = rm.occurrences_at(byte_func);
            assert!(!func_occs.is_empty(),
                "Function 'A' at line 1 should have ref group");

            // 3. Function 'A' group should include call A() at line 5
            let func_lines: Vec<usize> = func_occs.iter()
                .map(|o| o.range.start.line).collect();
            assert!(func_lines.contains(&5),
                "Function 'A' group should include call at line 5, got {:?}", func_lines);

            // 4. Local `A` at line 2, char 18
            let byte_local = Position { line: 2, character: 18 }
                .to_byte_offset(&rope).unwrap();
            let local_occs = rm.occurrences_at(byte_local);
            assert!(!local_occs.is_empty(),
                "Local 'A' at line 2 should have ref group");

            // 5. `set A = 21` at line 3 — A is at char 8
            let byte_set = Position { line: 3, character: 8 }
                .to_byte_offset(&rope).unwrap();
            let set_occs = rm.occurrences_at(byte_set);
            assert!(!set_occs.is_empty(),
                "set 'A' at line 3 should have ref group");

            // 6. set A should be in the SAME group as local A, NOT global or function A
            let local_key = rm.decl_key_at(byte_local);
            let set_key = rm.decl_key_at(byte_set);
            assert_eq!(local_key, set_key,
                "set A and local A should be in the same ref group");

            // 7. local A group should NOT include the top-level VarStmt A or function A
            let local_lines: Vec<usize> = local_occs.iter()
                .map(|o| o.range.start.line).collect();
            assert!(!local_lines.contains(&0),
                "Local A group should NOT contain line 0 (VarStmt)");
            assert!(!local_lines.contains(&1),
                "Local A group should NOT contain line 1 (function decl)");
        });
    }

    #[test]
    fn ref_click_every_position() {
        // All A positions should be reachable from RefMap
        let src = "\
real A = 33
function A takes nothing returns nothing
    local integer A = 33
    set A = 21
endfunction
call A(A + A(A))
";
        let rope = Rope::from(src);
        with_cursor(src, |c| {
            let rm = ref_map_from(c, &rope);

            // Each (line, char, expected_group_description)
            let positions = vec![
                (0, 5, "VarStmt A"),          // real A = 33
                (1, 9, "function A decl"),     // function A
                (2, 18, "local A decl"),        // local integer A
                (3, 8, "set A ref"),            // set A = 21
                (5, 5, "call A"),              // call A(...)
                (5, 7, "arg A (1st)"),         // call A(A + ...)
                (5, 11, "arg A(A) func call"),  // call A(... + A(...))
                (5, 13, "arg innermost A"),     // call A(... + A(A))
            ];

            for (line, ch, desc) in &positions {
                let pos = Position { line: *line, character: *ch };
                let byte = pos.to_byte_offset(&rope);
                assert!(byte.is_some(),
                    "{}: Position ({},{}) → no byte offset", desc, line, ch);
                let byte = byte.unwrap();
                let key = rm.decl_key_at(byte);
                assert!(key.is_some(),
                    "{}: byte {} → no decl_key. spans: {:?}",
                    desc, byte,
                    rm.spans.iter()
                        .map(|s| (s.start_byte, s.end_byte, s.decl_key))
                        .collect::<Vec<_>>());
                let occs = rm.occurrences_at(byte);
                assert!(!occs.is_empty(),
                    "{}: no occurrences at byte {}", desc, byte);
            }
        });
    }

    #[test]
    fn link_bare_assignment_without_set_keyword() {
        // `A = 21` inside a function body (no `set` keyword) should still
        // link to the local variable declaration.
        let src = "\
function F takes nothing returns nothing
    local integer A = 33
    A = 21
endfunction
";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "A");
            // 1 decl (local) + 1 write (bare assignment)
            assert_eq!(occs.len(), 2, "A: 1 local decl + 1 bare assignment");
            assert!(occs[0].is_decl);
            assert!(!occs[1].is_decl);
        });
    }

    #[test]
    fn link_func_decl_and_call() {
        let src = "\
function Foo takes nothing returns nothing
endfunction
call Foo()
";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "Foo");
            assert_eq!(occs.len(), 2, "Foo: 1 decl + 1 call");
            assert!(occs[0].is_decl);
            assert!(!occs[1].is_decl);
            // Both should be local keys
            for &key in c.ref_groups.keys() {
                assert!(key < EXTERNAL_KEY_BASE);
            }
        });
    }

    #[test]
    fn link_native_decl_and_call() {
        let src = "\
native MyNative takes nothing returns nothing
call MyNative()
";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "MyNative");
            assert_eq!(occs.len(), 2, "MyNative: 1 decl + 1 call");
            assert!(occs[0].is_decl);
            assert!(!occs[1].is_decl);
        });
    }

    #[test]
    fn link_global_var_used_in_function() {
        let src = "\
globals
    integer count = 0
endglobals
function Inc takes nothing returns nothing
    set count = count + 1
endfunction
";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "count");
            // 1 decl (globals) + 2 refs (set target + rhs read)
            assert_eq!(occs.len(), 3, "count: 1 decl + 1 write + 1 read, got {:?}",
                occs.iter().map(|o| (o.range.start.line, o.is_decl)).collect::<Vec<_>>());
            assert!(occs[0].is_decl, "first should be the declaration");
        });
    }

    #[test]
    fn link_type_decl_and_references() {
        let src = "\
type agent extends handle
globals
    agent a
endglobals
function F takes agent x returns agent
endfunction
";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "agent");
            // 1 decl (type name) + 3 refs (globals type, param type, return type)
            assert_eq!(occs.len(), 4, "agent: 1 decl + 3 refs, got {:?}",
                occs.iter().map(|o| (o.range.start.line, o.is_decl)).collect::<Vec<_>>());
            assert!(occs[0].is_decl);
        });
    }

    #[test]
    fn link_type_extends_base() {
        let src = "\
type handle extends agent
type unit extends handle
";
        with_cursor(src, |c| {
            let (_, handle_occs) = find_group(c, "handle");
            // 1 decl (first line) + 1 ref (extends handle in second line)
            assert_eq!(handle_occs.len(), 2, "handle: 1 decl + 1 extends ref");
            assert!(handle_occs[0].is_decl);
            assert!(!handle_occs[1].is_decl);
        });
    }

    #[test]
    fn link_local_shadows_global() {
        let src = "\
globals
    integer x = 10
endglobals
function F takes nothing returns nothing
    local integer x = 20
    set x = 30
endfunction
";
        with_cursor(src, |c| {
            let groups = find_groups(c, "x");
            assert_eq!(groups.len(), 2, "should have 2 separate groups for 'x' (global + local)");

            // Global group should have exactly 1 occurrence (the declaration)
            // — nobody reads it because local shadows inside the function
            let global_group = groups.iter()
                .find(|(_, occs)| occs.iter().any(|o| o.range.start.line == 1))
                .expect("should have a group with occurrence on line 1 (global)");
            assert_eq!(global_group.1.len(), 1, "global x: only 1 decl occurrence");
            assert!(global_group.1[0].is_decl);

            // Local group: 1 decl (line 4) + 1 set (line 5)
            let local_group = groups.iter()
                .find(|(_, occs)| occs.iter().any(|o| o.range.start.line == 4))
                .expect("should have a group with occurrence on line 4 (local)");
            let local_lines: Vec<_> = local_group.1.iter().map(|o| o.range.start.line).collect();
            assert!(local_lines.contains(&4), "local group should contain decl (line 4)");
            assert!(local_lines.contains(&5), "local group should contain set (line 5)");
            assert!(!local_lines.contains(&1), "local group should NOT contain line 1 (global)");
        });
    }

    #[test]
    fn link_param_used_in_body() {
        let src = "\
function F takes integer n returns integer
    return n
endfunction
";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "n");
            assert_eq!(occs.len(), 2, "n: 1 param decl + 1 return ref");
            assert!(occs[0].is_decl);
            assert!(!occs[1].is_decl);
        });
    }

    #[test]
    fn link_func_and_var_same_name_separate() {
        let src = "\
globals
    integer A = 1
endglobals
function A takes nothing returns nothing
endfunction
function Main takes nothing returns nothing
    set A = 10
    call A()
endfunction
";
        with_cursor(src, |c| {
            let groups = find_groups(c, "A");
            // Should be 2 separate groups: one for variable, one for function
            assert_eq!(groups.len(), 2,
                "var A and function A should be in separate groups (different namespaces)");

            // Variable group: decl (line 1) + set (line 6)
            let var_group = groups.iter()
                .find(|(_, occs)| occs.iter().any(|o| o.range.start.line == 1))
                .expect("should have A var group at line 1");
            let var_lines: Vec<_> = var_group.1.iter().map(|o| o.range.start.line).collect();
            assert!(var_lines.contains(&1), "var group: decl at line 1");
            assert!(var_lines.contains(&6), "var group: set at line 6");

            // Function group: decl (line 3) + call (line 7)
            let func_group = groups.iter()
                .find(|(_, occs)| occs.iter().any(|o| o.range.start.line == 3))
                .expect("should have A func group at line 3");
            let func_lines: Vec<_> = func_group.1.iter().map(|o| o.range.start.line).collect();
            assert!(func_lines.contains(&3), "func group: decl at line 3");
            assert!(func_lines.contains(&7), "func group: call at line 7");
        });
    }

    #[test]
    fn link_standalone_multiple_calls_share_group() {
        let src = "\
function F takes nothing returns nothing
    call Unknown()
    call Unknown()
    call Unknown()
endfunction
";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "Unknown");
            assert_eq!(occs.len(), 3, "3 calls to Unknown should share one group");
            assert!(occs[0].is_decl, "first occurrence is the 'self-declaration'");
            assert!(!occs[1].is_decl);
            assert!(!occs[2].is_decl);
        });
    }

    #[test]
    fn link_standalone_var_set_and_read_share_group() {
        let src = "\
function F takes nothing returns nothing
    set udg_x = 1
    set udg_x = udg_x + 1
endfunction
";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "udg_x");
            // 3 occurrences: set(write) + set(write) + read in expression
            assert_eq!(occs.len(), 3, "udg_x: 2 set + 1 read = 3 occurrences, got {:?}",
                occs.iter().map(|o| (o.range.start.line, o.is_decl)).collect::<Vec<_>>());
        });
    }

    #[test]
    fn link_standalone_single_ref_is_self_decl() {
        let src = "call Xyz()\n";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "Xyz");
            assert_eq!(occs.len(), 1);
            assert!(occs[0].is_decl, "single standalone ref is its own decl");
        });
    }

    #[test]
    fn link_import_func_resolves() {
        let src = "call Bar()\ncall Bar()\n";
        let origin = Url::parse("file:///lib/common.j").unwrap();
        let imported = vec![ImportedSymbol {
            origin_uri: origin.clone(),
            name: "Bar".into(),
            kind: ImportedKind::Func, origin_decl_key: None, return_type: None, type_name: None,
        }];
        with_cursor_imported(src, &imported, |c| {
            let ext_key = *c.ref_groups.keys()
                .find(|&&k| k >= EXTERNAL_KEY_BASE)
                .expect("should have an external key for Bar");
            assert_eq!(c.ref_names[&ext_key], "Bar");
            assert_eq!(c.external_decls[&ext_key].origins[0].uri, origin);
            let occs = &c.ref_groups[&ext_key];
            assert_eq!(occs.len(), 2, "both calls to Bar should be in the external group");
            assert!(!occs[0].is_decl, "external refs are not declarations");
            assert!(!occs[1].is_decl);
        });
    }

    #[test]
    fn link_import_var_resolves() {
        let src = "\
function F takes nothing returns nothing
    set bj_lastCreatedUnit = null
endfunction
";
        let origin = Url::parse("file:///lib/blizzard.j").unwrap();
        let imported = vec![ImportedSymbol {
            origin_uri: origin.clone(),
            name: "bj_lastCreatedUnit".into(),
            kind: ImportedKind::Var, origin_decl_key: None, return_type: None, type_name: None,
        }];
        with_cursor_imported(src, &imported, |c| {
            let ext_key = *c.ref_groups.keys()
                .find(|&&k| k >= EXTERNAL_KEY_BASE
                    && c.ref_names.get(&k).map(|n| n == "bj_lastCreatedUnit").unwrap_or(false))
                .expect("should have external key for bj_lastCreatedUnit");
            assert_eq!(c.external_decls[&ext_key].origins[0].uri, origin);
        });
    }

    #[test]
    fn link_import_type_resolves() {
        let src = "\
function F takes nothing returns nothing
    local group g = null
    local unit u = null
endfunction
";
        let origin = Url::parse("file:///lib/common.j").unwrap();
        let imported = vec![
            ImportedSymbol { origin_uri: origin.clone(), name: "group".into(), kind: ImportedKind::Var, origin_decl_key: None, return_type: None, type_name: None },
            ImportedSymbol { origin_uri: origin.clone(), name: "unit".into(),  kind: ImportedKind::Var, origin_decl_key: None, return_type: None, type_name: None },
        ];
        with_cursor_imported(src, &imported, |c| {
            for type_name in &["group", "unit"] {
                let key = c.ref_groups.keys()
                    .find(|&&k| k >= EXTERNAL_KEY_BASE
                        && c.ref_names.get(&k).map(|n| n.as_str() == *type_name).unwrap_or(false));
                assert!(key.is_some(),
                    "type {:?} should resolve as an imported symbol", type_name);
                let key = *key.unwrap();
                assert_eq!(c.external_decls[&key].origins[0].uri, origin);
            }
        });
    }

    #[test]
    fn link_primitive_types_not_in_refs() {
        let src = "\
globals
    integer x = 0
    real y = 0.0
    boolean b = true
    string s = \"hi\"
endglobals
";
        with_cursor(src, |c| {
            for prim in &["integer", "real", "boolean", "string"] {
                let groups = find_groups(c, prim);
                assert!(groups.is_empty(),
                    "primitive type {:?} should NOT appear in ref_groups", prim);
            }
        });
    }

    #[test]
    fn link_local_decl_shadows_import() {
        let src = "\
function A takes nothing returns nothing
endfunction
call A()
";
        let origin = Url::parse("file:///lib/common.j").unwrap();
        let imported = vec![ImportedSymbol {
            origin_uri: origin,
            name: "A".into(),
            kind: ImportedKind::Func, origin_decl_key: None, return_type: None, type_name: None,
        }];
        with_cursor_imported(src, &imported, |c| {
            let ext_count = c.ref_groups.keys().filter(|&&k| k >= EXTERNAL_KEY_BASE).count();
            assert_eq!(ext_count, 0, "local A should shadow the import");
            let (_, occs) = find_group(c, "A");
            assert_eq!(occs.len(), 2, "A: 1 decl + 1 call");
        });
    }

    #[test]
    fn link_import_func_and_var_same_name_separate() {
        // Simulates ass.j: `A = 44` (var ref) + `call A()` (func ref)
        // where anal.j exports both `real A` (Var) and `function A` (Func).
        let src = "\
A = 44
call A()
call A()
";
        let origin = Url::parse("file:///test/anal.j").unwrap();
        let imported = vec![
            ImportedSymbol {
                origin_uri: origin.clone(),
                name: "A".into(),
                kind: ImportedKind::Var,
                origin_decl_key: None, return_type: None, type_name: None,
            },
            ImportedSymbol {
                origin_uri: origin.clone(),
                name: "A".into(),
                kind: ImportedKind::Func,
                origin_decl_key: None, return_type: None, type_name: None,
            },
        ];
        with_cursor_imported(src, &imported, |c| {
            let ext_keys: Vec<_> = c.ref_groups.keys()
                .filter(|&&k| k >= EXTERNAL_KEY_BASE
                    && c.ref_names.get(&k).map(|n| n == "A").unwrap_or(false))
                .copied()
                .collect();
            assert_eq!(ext_keys.len(), 2,
                "A should have two separate external groups (var + func), got {:?}",
                ext_keys);

            // One group should have 1 occurrence (the set), the other 2 (the calls)
            let mut sizes: Vec<_> = ext_keys.iter()
                .map(|k| c.ref_groups[k].len())
                .collect();
            sizes.sort();
            assert_eq!(sizes, vec![1, 2],
                "var group: 1 (set A=44), func group: 2 (call A() x2)");
        });
    }

    #[test]
    fn link_import_real_ass_j_scenario() {
        // Exact content of ass.j (minus directives which become SetDir)
        let src = "\
//set hint ref type

set a = \" \\\"\"+122

A = 44

function B takes nothing returns nothing
    call B1()
endfunction

function E takes nothing returns nothing

    call B()

endfunction

call A()
call A()

call E()

";
        let origin = Url::parse("file:///test/anal.j").unwrap();
        let imported = vec![
            ImportedSymbol {
                origin_uri: origin.clone(),
                name: "A".into(),
                kind: ImportedKind::Var,
                origin_decl_key: None, return_type: None, type_name: None,
            },
            ImportedSymbol {
                origin_uri: origin.clone(),
                name: "A".into(),
                kind: ImportedKind::Func,
                origin_decl_key: None, return_type: None, type_name: None,
            },
        ];
        with_cursor_imported(src, &imported, |c| {
            let ext_keys: Vec<_> = c.ref_groups.keys()
                .filter(|&&k| k >= EXTERNAL_KEY_BASE
                    && c.ref_names.get(&k).map(|n| n == "A").unwrap_or(false))
                .copied()
                .collect();
            assert_eq!(ext_keys.len(), 2,
                "A should have two external groups (var + func) in real ass.j. \
                 Groups named A: {:?}",
                find_groups(c, "A").iter()
                    .map(|&(&k, ref occs)| (k, k >= EXTERNAL_KEY_BASE, occs.len()))
                    .collect::<Vec<_>>());

            let mut sizes: Vec<_> = ext_keys.iter()
                .map(|k| c.ref_groups[k].len())
                .collect();
            sizes.sort();
            assert_eq!(sizes, vec![1, 2],
                "var group: 1 (A=44), func group: 2 (call A() x2)");
        });
    }

    #[test]
    fn link_refmap_spans_cover_all_ids() {
        let src = "\
type handle extends agent
type mytype extends handle
globals
    mytype g = null
endglobals
function F takes mytype x returns nothing
    local integer y = 0
    set y = y + 1
    call DoStuff(x, y)
endfunction
call F(g)
";
        let rope = Rope::from(src);
        with_cursor(src, |c| {
            let rm = ref_map_from(c, &rope);

            // Every identifier position should have a span
            // Note: primitive types (integer, etc.) are silently skipped
            let checks = vec![
                (0, 5,  "handle decl"),
                (1, 5,  "mytype decl"),
                (1, 20, "handle base ref"),  // `handle` is declared at line 0
                (3, 11, "g decl"),
                (5, 9,  "F decl"),
                (6, 18, "y decl"),
                (7, 8,  "y set write"),
                (7, 12, "y read"),
                (8, 9,  "DoStuff call"),
                (8, 17, "x read"),
                (8, 20, "y read in args"),
                (10, 5,  "F call"),
                (10, 7,  "g read in call"),
            ];
            for (line, ch, desc) in &checks {
                assert_span_at(&rm, &rope, *line, *ch, desc);
            }
        });
    }

    #[test]
    fn link_refmap_definition_at() {
        let src = "\
globals
    integer x = 0
endglobals
function F takes nothing returns nothing
    set x = 1
endfunction
";
        let rope = Rope::from(src);
        with_cursor(src, |c| {
            let rm = ref_map_from(c, &rope);
            // `x` in `set x = 1` → definition should be at line 1
            let byte = Position { line: 4, character: 8 }.to_byte_offset(&rope).unwrap();
            let defs = rm.definitions_at(byte);
            assert_eq!(defs.len(), 1, "should have exactly one definition for x");
            assert_eq!(defs[0].range.start.line, 1, "definition should be on line 1 (globals)");
            assert!(defs[0].is_decl);
        });
    }

    #[test]
    fn link_refmap_occurrences_at() {
        let src = "\
function F takes nothing returns nothing
endfunction
function G takes nothing returns nothing
    call F()
endfunction
call F()
";
        let rope = Rope::from(src);
        with_cursor(src, |c| {
            let rm = ref_map_from(c, &rope);
            // Click on `F` at decl → should return 3 occurrences (decl + 2 calls)
            let byte = Position { line: 0, character: 9 }.to_byte_offset(&rope).unwrap();
            let all = rm.occurrences_at(byte);
            assert_eq!(all.len(), 3, "F: 1 decl + 2 calls");
            let decls: Vec<_> = all.iter().filter(|o| o.is_decl).collect();
            assert_eq!(decls.len(), 1, "exactly 1 declaration");
            let refs: Vec<_> = all.iter().filter(|o| !o.is_decl).collect();
            assert_eq!(refs.len(), 2, "exactly 2 references");
        });
    }

    #[test]
    fn link_refmap_name_at() {
        let src = "\
function LongFunctionName takes nothing returns nothing
endfunction
call LongFunctionName()
";
        let rope = Rope::from(src);
        with_cursor(src, |c| {
            let rm = ref_map_from(c, &rope);
            // Click on `LongFunctionName` at call
            let byte = Position { line: 2, character: 5 }.to_byte_offset(&rope).unwrap();
            let name = rm.name_at(byte);
            assert_eq!(name, Some("LongFunctionName"));
        });
    }

    #[test]
    fn link_func_call_in_expression() {
        let src = "\
function Add takes integer a, integer b returns integer
    return a
endfunction
function Main takes nothing returns nothing
    local integer r = Add(1, Add(2, 3))
endfunction
";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "Add");
            // 1 decl + 2 calls (outer + inner)
            assert_eq!(occs.len(), 3, "Add: 1 decl + 2 calls");
        });
    }

    #[test]
    fn link_func_ref_expression() {
        let src = "\
function Callback takes nothing returns nothing
endfunction
function Main takes nothing returns nothing
    local code c = function Callback
endfunction
";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "Callback");
            assert_eq!(occs.len(), 2, "Callback: 1 decl + 1 function ref");
        });
    }

    #[test]
    fn link_var_in_loop_and_if() {
        let src = "\
function F takes nothing returns nothing
    local integer i = 0
    loop
        exitwhen i >= 10
        if i > 5 then
            set i = i + 2
        endif
        set i = i + 1
    endloop
endfunction
";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "i");
            // 1 decl + 6 refs (exitwhen read, if read, set write, set read, set write, set read)
            assert!(occs.len() >= 7, "i: 1 decl + at least 6 refs, got {}", occs.len());
            assert!(occs[0].is_decl, "first should be decl");
        });
    }

    #[test]
    fn link_constant_var() {
        let src = "\
globals
    constant integer MAX = 100
endglobals
function F takes nothing returns nothing
    local integer x = MAX
endfunction
";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "MAX");
            assert_eq!(occs.len(), 2, "MAX: 1 decl + 1 read");
            assert!(occs[0].is_decl);
        });
    }

    #[test]
    fn link_array_set_and_read() {
        let src = "\
globals
    integer array arr
endglobals
function F takes nothing returns nothing
    set arr[0] = 1
    local integer v = arr[0]
endfunction
";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "arr");
            assert_eq!(occs.len(), 3, "arr: 1 decl + 1 set + 1 read");
        });
    }

    #[test]
    fn link_mutual_calls() {
        let src = "\
function A takes nothing returns nothing
    call B()
endfunction
function B takes nothing returns nothing
    call A()
endfunction
";
        with_cursor(src, |c| {
            let (_, a_occs) = find_group(c, "A");
            let (_, b_occs) = find_group(c, "B");
            assert_eq!(a_occs.len(), 2, "A: 1 decl + 1 call from B");
            assert_eq!(b_occs.len(), 2, "B: 1 decl + 1 call from A");
        });
    }

    #[test]
    fn cross_file_imported_var_bare_set() {
        // ass.j: top-level bare assignment to imported variable
        let src = "A = 44\n";
        let origin = Url::parse("file:///test/anal.j").unwrap();
        let imported = vec![ImportedSymbol {
            origin_uri: origin.clone(),
            name: "A".into(),
            kind: ImportedKind::Var,
            origin_decl_key: None, return_type: None, type_name: None,
        }];
        with_cursor_imported(src, &imported, |c| {
            // `A` should resolve to the external group
            let ext_keys: Vec<_> = c
                .ref_groups
                .keys()
                .filter(|&&k| k >= EXTERNAL_KEY_BASE)
                .copied()
                .collect();
            assert!(
                !ext_keys.is_empty(),
                "bare `A = 44` should produce an external ref group for imported var A.\n\
                 ref_names: {:?}\n\
                 groups: {:?}",
                c.ref_names,
                c.ref_groups
                    .iter()
                    .map(|(k, v)| (k, c.ref_names.get(k), v.len()))
                    .collect::<Vec<_>>()
            );
            let ext_a = ext_keys
                .iter()
                .find(|k| c.ref_names.get(k).map(|n| n == "A").unwrap_or(false));
            assert!(
                ext_a.is_some(),
                "expected external group named 'A', got names: {:?}",
                ext_keys
                    .iter()
                    .map(|k| c.ref_names.get(k))
                    .collect::<Vec<_>>()
            );
            let key = *ext_a.unwrap();
            assert_eq!(c.external_decls[&key].origins[0].uri, origin);
            let occs = &c.ref_groups[&key];
            assert_eq!(occs.len(), 1, "1 ref (the bare set)");
            assert!(!occs[0].is_decl, "external ref should not be a declaration");
        });
    }

    #[test]
    fn cross_file_imported_func_call() {
        // ass.j: call to imported function
        let src = "call A()\n";
        let origin = Url::parse("file:///test/anal.j").unwrap();
        let imported = vec![ImportedSymbol {
            origin_uri: origin.clone(),
            name: "A".into(),
            kind: ImportedKind::Func,
            origin_decl_key: None, return_type: None, type_name: None,
        }];
        with_cursor_imported(src, &imported, |c| {
            let ext_keys: Vec<_> = c
                .ref_groups
                .keys()
                .filter(|&&k| k >= EXTERNAL_KEY_BASE)
                .copied()
                .collect();
            assert!(
                !ext_keys.is_empty(),
                "`call A()` should produce an external ref group for imported func A.\n\
                 ref_names: {:?}\n\
                 groups: {:?}",
                c.ref_names,
                c.ref_groups
                    .iter()
                    .map(|(k, v)| (k, c.ref_names.get(k), v.len()))
                    .collect::<Vec<_>>()
            );
            let ext_a = ext_keys
                .iter()
                .find(|k| c.ref_names.get(k).map(|n| n == "A").unwrap_or(false));
            assert!(
                ext_a.is_some(),
                "expected external group named 'A'"
            );
            let key = *ext_a.unwrap();
            assert_eq!(c.external_decls[&key].origins[0].uri, origin);
        });
    }

    #[test]
    fn cross_file_both_var_and_func() {
        // ass.j: uses A as both a variable and a function
        let src = "\
A = 44
call A()
call A()
";
        let origin = Url::parse("file:///test/anal.j").unwrap();
        let imported = vec![
            ImportedSymbol {
                origin_uri: origin.clone(),
                name: "A".into(),
                kind: ImportedKind::Var,
                origin_decl_key: Some(0), return_type: None, type_name: None,
            },
            ImportedSymbol {
                origin_uri: origin.clone(),
                name: "A".into(),
                kind: ImportedKind::Func,
                origin_decl_key: Some(1), return_type: None, type_name: None,
            },
        ];
        with_cursor_imported(src, &imported, |c| {
            // There should be TWO external groups for A:
            // one for the Var namespace, one for the Func namespace.
            let ext_a_keys: Vec<_> = c
                .ref_groups
                .keys()
                .filter(|&&k| k >= EXTERNAL_KEY_BASE
                    && c.ref_names.get(&k).map(|n| n == "A").unwrap_or(false))
                .copied()
                .collect();
            assert_eq!(
                ext_a_keys.len(),
                2,
                "expected 2 external groups for 'A' (var + func), got {}.\n\
                 all ref_names: {:?}\n\
                 all groups: {:?}",
                ext_a_keys.len(),
                c.ref_names,
                c.ref_groups
                    .iter()
                    .map(|(k, v)| (k, c.ref_names.get(k), v.len()))
                    .collect::<Vec<_>>()
            );

            // The var group should have 1 occurrence (A = 44)
            // The func group should have 2 occurrences (call A() × 2)
            let mut occ_counts: Vec<usize> = ext_a_keys.iter()
                .map(|k| c.ref_groups[k].len())
                .collect();
            occ_counts.sort();
            assert_eq!(
                occ_counts,
                vec![1, 2],
                "var group: 1 (set A=44), func group: 2 (call A() x2)");
        });
    }

    #[test]
    fn forward_ref_set_before_declaration() {
        // `B = 3` appears BEFORE `boolean B` declaration.
        // Phase 2 should link B via forward local lookup — no "Undeclared".
        let src = "\
function F takes nothing returns nothing
    set B = 3
endfunction
globals
    integer B = 0
endglobals
";
        with_cursor(src, |c| {
            let undecl: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Undeclared"))
                .collect();
            assert!(
                undecl.is_empty(),
                "No undeclared diagnostics expected for forward ref B, got: {:?}", undecl
            );
            // B should be in a single group with 2 occurrences (1 decl + 1 ref).
            let (_, occs) = find_group(c, "B");
            assert_eq!(occs.len(), 2, "B: 1 decl + 1 forward ref");
        });
    }

    #[test]
    fn forward_ref_call_before_function_decl() {
        // `call Foo()` appears BEFORE `function Foo`.
        // Phase 2 should resolve via forward local (global scope).
        let src = "\
call Foo()
function Foo takes nothing returns nothing
endfunction
";
        with_cursor(src, |c| {
            let undecl: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Undeclared"))
                .collect();
            assert!(
                undecl.is_empty(),
                "No undeclared diagnostics expected for forward call to Foo, got: {:?}", undecl
            );
            let (_, occs) = find_group(c, "Foo");
            assert_eq!(occs.len(), 2, "Foo: 1 decl + 1 forward call");
        });
    }

    #[test]
    fn forward_ref_varstmt_before_use() {
        // Top-level: `B = 3` before `boolean B`.
        // Phase 1 sees `B = 3` first (unresolved), then declares `boolean B`.
        // Phase 2 links B via forward local.
        let src = "\
set B = 3
boolean B
";
        with_cursor(src, |c| {
            let undecl: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Undeclared"))
                .collect();
            assert!(
                undecl.is_empty(),
                "No undeclared diagnostics expected for forward ref B, got: {:?}", undecl
            );
        });
    }

    #[test]
    fn cross_file_import_resolves_function() {
        // `call CreateUnit()` where CreateUnit is an imported function.
        // Phase 2 should resolve via imported symbols — no "Undeclared".
        let src = "call CreateUnit()\n";
        let origin = Url::parse("file:///common.j").unwrap();
        let imported = vec![ImportedSymbol {
            origin_uri: origin,
            name: "CreateUnit".into(),
            kind: ImportedKind::Func,
            origin_decl_key: Some(42), return_type: None, type_name: None,
        }];
        with_cursor_imported(src, &imported, |c| {
            let undecl: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Undeclared"))
                .collect();
            assert!(
                undecl.is_empty(),
                "CreateUnit should resolve via import, got: {:?}", undecl
            );
            // Should have an external group for CreateUnit
            let groups = find_groups(c, "CreateUnit");
            assert_eq!(groups.len(), 1, "CreateUnit: exactly 1 group");
            let (&key, _) = groups[0];
            assert!(
                key >= EXTERNAL_KEY_BASE,
                "CreateUnit key should be external (>= {}), got {}",
                EXTERNAL_KEY_BASE, key
            );
        });
    }

    #[test]
    fn cross_file_import_resolves_variable() {
        // `set x = G` where G is an imported global variable.
        let src = "\
integer x = G
";
        let origin = Url::parse("file:///globals.j").unwrap();
        let imported = vec![ImportedSymbol {
            origin_uri: origin,
            name: "G".into(),
            kind: ImportedKind::Var,
            origin_decl_key: Some(10), return_type: None, type_name: None,
        }];
        with_cursor_imported(src, &imported, |c| {
            let undecl: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Undeclared") && d.message.contains("`G`"))
                .collect();
            assert!(
                undecl.is_empty(),
                "G should resolve via import, got: {:?}", undecl
            );
        });
    }

    #[test]
    fn cross_file_missing_import_emits_undeclared() {
        // `call Missing()` with no matching import — should get "Undeclared".
        let src = "call Missing()\n";
        with_cursor_imported(src, &[], |c| {
            let undecl: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Undeclared") && d.message.contains("`Missing`"))
                .collect();
            assert!(
                !undecl.is_empty(),
                "Expected 'Undeclared function Missing' diagnostic, got: {:?}", c.diagnostics
            );
        });
    }

    #[test]
    fn forward_ref_and_import_coexist() {
        // `call Foo()` appears before `function Foo` declaration.
        // An import also provides `Bar`.
        // Both should resolve — Foo via forward local, Bar via import.
        let src = "\
call Foo()
call Bar()
function Foo takes nothing returns nothing
endfunction
";
        let origin = Url::parse("file:///lib.j").unwrap();
        let imported = vec![ImportedSymbol {
            origin_uri: origin,
            name: "Bar".into(),
            kind: ImportedKind::Func,
            origin_decl_key: Some(99), return_type: None, type_name: None,
        }];
        with_cursor_imported(src, &imported, |c| {
            let undecl: Vec<_> = c.diagnostics.iter()
                .filter(|d| d.message.contains("Undeclared"))
                .collect();
            assert!(
                undecl.is_empty(),
                "Both Foo (forward) and Bar (import) should resolve, got: {:?}", undecl
            );
            // Foo should be a local group
            let foo_groups = find_groups(c, "Foo");
            assert_eq!(foo_groups.len(), 1);
            let (&foo_key, _) = foo_groups[0];
            assert!(foo_key < EXTERNAL_KEY_BASE, "Foo should be local");
            // Bar should be an external group
            let bar_groups = find_groups(c, "Bar");
            assert_eq!(bar_groups.len(), 1);
            let (&bar_key, _) = bar_groups[0];
            assert!(bar_key >= EXTERNAL_KEY_BASE, "Bar should be external");
        });
    }
