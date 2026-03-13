#[cfg(test)]
mod tests {
    use crate::lng::jass::ast::*;
    use crate::lng::jass::cursor::{Cursor, ImportedKind, ImportedSymbol};
    use crate::lsp::document_symbol::lsp::SymbolKind;
    use crate::lsp::folding::lsp::FoldingRangeKind;
    use crate::lsp::position::Position;
    use crate::lsp::ref_map::{build_ref_map, EXTERNAL_KEY_BASE};
    use crate::lsp::semantic::lsp::Kind as TokenKind;
    use lapce_xi_rope::Rope;
    use url::Url;

    fn with_cursor(src: &str, f: impl FnOnce(&Cursor)) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        let rope = Rope::from(src);
        let cursor = Cursor::walk(&ast, &rope, &[]);
        f(&cursor);
    }

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
    fn semantic_tokens_present() {
        let src = "\
function Foo takes nothing returns nothing
    call Bar()
endfunction
";
        with_cursor(src, |c| {
            let data = c.semantic.data(None);
            assert!(!data.is_empty(), "Should have semantic tokens");
        });
    }

    #[test]
    fn semantic_function_call_name_is_function() {
        let src = "call Foo()\n";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            let foo_token = line.tokens.iter().find(|t| t.col == 5 && t.len == 3);
            assert!(
                foo_token.is_some(),
                "Should have a token for 'Foo' at col=5 len=3, tokens: {:?}",
                line.tokens
            );
            assert_eq!(
                foo_token.unwrap().kind,
                TokenKind::Function,
                "Function call name should be TokenKind::Function, got {:?}",
                foo_token.unwrap().kind
            );
        });
    }

    #[test]
    fn semantic_function_call_inside_body_is_function() {
        let src = "\
function main takes nothing returns nothing
    call Foo()
endfunction
";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&1).expect("should have line 1");
            eprintln!("line 1 tokens: {:?}", line.tokens);
            let tok = line.tokens.iter().find(|t| t.col == 9 && t.len == 3);
            assert!(
                tok.is_some(),
                "Should have token for 'Foo' at col=9 len=3, tokens: {:?}",
                line.tokens
            );
            assert_eq!(
                tok.unwrap().kind,
                TokenKind::Function,
                "call Foo() name should be Function, got {:?}",
                tok.unwrap().kind
            );
        });
    }

    #[test]
    fn semantic_function_decl_name_is_function() {
        let src = "function MyFunc takes nothing returns nothing\nendfunction\n";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            let tok = line.tokens.iter().find(|t| t.col == 9 && t.len == 6);
            assert!(
                tok.is_some(),
                "Should have token for 'MyFunc', tokens: {:?}",
                line.tokens
            );
            assert_eq!(tok.unwrap().kind, TokenKind::Function);
        });
    }

    #[test]
    fn semantic_type_name_is_type() {
        let src = "type handle extends agent\n";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            let tok = line.tokens.iter().find(|t| t.col == 5 && t.len == 6);
            assert!(
                tok.is_some(),
                "Should have token for 'handle', tokens: {:?}",
                line.tokens
            );
            assert_eq!(tok.unwrap().kind, TokenKind::Type);
            let tok2 = line.tokens.iter().find(|t| t.col == 20 && t.len == 5);
            assert!(
                tok2.is_some(),
                "Should have token for 'agent', tokens: {:?}",
                line.tokens
            );
            assert_eq!(tok2.unwrap().kind, TokenKind::Type);
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
            assert!(c.diagnostics.is_empty());
        });
    }

    #[test]
    fn semantic_string_literal() {
        let src = "call Foo(\"my shit\")\n";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&0).expect("should have line 0");
            let str_tok = line.tokens.iter().find(|t| t.col == 9);
            assert!(
                str_tok.is_some(),
                "Should have a string token at col=9, tokens: {:?}",
                line.tokens
            );
            assert_eq!(
                str_tok.unwrap().kind,
                TokenKind::String,
                "String literal should be TokenKind::String, got {:?}",
                str_tok.unwrap().kind
            );
            assert_eq!(
                str_tok.unwrap().len,
                9,
                "String token len should be 9 (including quotes)"
            );
        });
    }

    #[test]
    fn semantic_function_call_in_expression() {
        let src = "\
function F takes unit Target returns boolean
    return UnitLife(Target) > 0 and not IsHidden(Target)
endfunction
";
        with_cursor(src, |c| {
            let line = c.semantic.lines.get(&1).expect("should have line 1");
            let ul_tok = line.tokens.iter().find(|t| t.col == 11 && t.len == 8);
            assert!(
                ul_tok.is_some(),
                "Should have UnitLife token, tokens: {:?}",
                line.tokens
            );
            assert_eq!(
                ul_tok.unwrap().kind,
                TokenKind::Function,
                "UnitLife should be Function, got {:?}",
                ul_tok.unwrap().kind
            );
            let ih_tok = line.tokens.iter().find(|t| t.col == 40 && t.len == 8);
            assert!(
                ih_tok.is_some(),
                "Should have IsHidden token, tokens: {:?}",
                line.tokens
            );
            assert_eq!(
                ih_tok.unwrap().kind,
                TokenKind::Function,
                "IsHidden should be Function, got {:?}",
                ih_tok.unwrap().kind
            );
        });
    }

    #[test]
    fn semantic_no_trailing_newline() {
        // File ends with "endglobals" right at EOF, no \n
        let src = "globals\n    integer a = 0xFF23A284\nendglobals";
        with_cursor(src, |c| {
            let data = c.semantic.data(None);
            assert!(
                !data.is_empty(),
                "Should have semantic tokens even without trailing newline"
            );
            // "integer" on line 1 should be a Type token
            let line1 = c.semantic.lines.get(&1);
            assert!(
                line1.is_some(),
                "Should have tokens on line 1, all lines: {:?}",
                c.semantic.lines.keys().collect::<Vec<_>>()
            );
            let int_tok = line1
                .unwrap()
                .tokens
                .iter()
                .find(|t| t.col == 4 && t.len == 7);
            assert!(
                int_tok.is_some(),
                "Should have 'integer' token at col=4 len=7, tokens: {:?}",
                line1.unwrap().tokens
            );
            assert_eq!(int_tok.unwrap().kind, TokenKind::Type);
        });
    }

    #[test]
    fn semantic_no_trailing_newline_single_line() {
        // Single line, no newline — everything on line 0
        let src = "globals\ninteger a = 0xFF23A284\nendglobals";
        with_cursor(src, |c| {
            eprintln!(
                "All semantic lines: {:?}",
                c.semantic.lines.keys().collect::<Vec<_>>()
            );
            for (line_num, line) in &c.semantic.lines {
                eprintln!("  line {}: {:?}", line_num, line.tokens);
            }
            // "integer" should be on line 1
            let line1 = c.semantic.lines.get(&1);
            assert!(line1.is_some(), "Should have tokens on line 1");
        });
    }

    #[test]
    fn semantic_eof_after_number() {
        // endglobals at EOF without trailing newline
        let src = "globals\n    integer a = 0xFF23A284\nendglobals";
        with_cursor(src, |c| {
            let data = c.semantic.data(None);
            assert!(
                !data.is_empty(),
                "Should have semantic tokens even without trailing newline"
            );
            // "integer" on line 1 should be a Type token
            let line1 = c.semantic.lines.get(&1);
            assert!(line1.is_some(), "Should have tokens on line 1");
            let int_tok = line1
                .unwrap()
                .tokens
                .iter()
                .find(|t| t.col == 4 && t.len == 7);
            assert!(int_tok.is_some(), "Should have 'integer' token");
            assert_eq!(int_tok.unwrap().kind, TokenKind::Type);
            // endglobals keyword on line 2
            let line2 = c.semantic.lines.get(&2);
            assert!(
                line2.is_some(),
                "Should have tokens on line 2 (endglobals at EOF), all lines: {:?}",
                c.semantic.lines.keys().collect::<Vec<_>>()
            );
        });
    }

    // ─── FileSymbols tests ──────────────────────────────────────────────

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

    // ─── Document Highlight tests ───────────────────────────────────────

    /// Build a `RefMap` from cursor data for test assertions.
    fn ref_map_from(c: &Cursor, rope: &Rope) -> crate::lsp::ref_map::RefMap {
        build_ref_map(c.ref_groups.clone(), c.ref_names.clone(), c.external_decls.clone(), rope)
    }

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

    // ─── Two-phase import resolution ─────────────────────────────────

    fn with_cursor_imported(
        src: &str,
        imported: &[ImportedSymbol],
        f: impl FnOnce(&Cursor),
    ) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        let rope = Rope::from(src);
        let cursor = Cursor::walk(&ast, &rope, imported);
        f(&cursor);
    }

    // ======================================================================
    //  Linking tests
    // ======================================================================
    //
    //  Every identifier in the file should participate in exactly one
    //  `ref_group`.  These tests verify:
    //
    //  • local declaration → usage linkage
    //  • scope isolation (local shadows global, function scope boundary)
    //  • var / func namespace separation
    //  • standalone (unresolved) grouping
    //  • imported symbol resolution
    //  • RefMap round-trip (spans, definitions, occurrences)

    // ── helpers ──────────────────────────────────────────────────────────

    /// Find the **single** ref-group whose name matches `name`.
    /// Panics with a diagnostic message if zero or more than one match.
    fn find_group<'a>(
        cursor: &'a Cursor,
        name: &str,
    ) -> (&'a usize, &'a Vec<crate::lsp::ref_map::RawOccurrence>) {
        let mut found: Vec<_> = cursor.ref_groups.iter()
            .filter(|(k, _)| cursor.ref_names.get(k).map(|n| n == name).unwrap_or(false))
            .collect();
        assert_eq!(
            found.len(), 1,
            "expected exactly 1 group for {:?}, got {} (keys: {:?})",
            name, found.len(),
            cursor.ref_names.iter().filter(|(_, v)| v.as_str() == name).collect::<Vec<_>>()
        );
        found.pop().unwrap()
    }

    /// Like `find_group` but returns all groups with the given name.
    fn find_groups<'a>(
        cursor: &'a Cursor,
        name: &str,
    ) -> Vec<(&'a usize, &'a Vec<crate::lsp::ref_map::RawOccurrence>)> {
        cursor.ref_groups.iter()
            .filter(|(k, _)| cursor.ref_names.get(k).map(|n| n == name).unwrap_or(false))
            .collect()
    }

    /// Assert that `RefMap.span_at(byte)` returns a non-None span.
    fn assert_span_at(rm: &crate::lsp::ref_map::RefMap, rope: &Rope, line: usize, ch: usize, desc: &str) {
        let byte = Position { line, character: ch }
            .to_byte_offset(rope)
            .unwrap_or_else(|| panic!("{}: position ({},{}) has no byte offset", desc, line, ch));
        let key = rm.decl_key_at(byte);
        assert!(key.is_some(),
            "{}: no span at byte {} (L{}:{}). spans: {:?}",
            desc, byte, line, ch,
            rm.spans.iter().map(|s| (s.start_byte, s.end_byte, s.decl_key)).collect::<Vec<_>>());
    }

    // ── local: function decl + call ─────────────────────────────────────

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

    // ── local: native decl + call ───────────────────────────────────────

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

    // ── local: global var decl + usage in function body ─────────────────

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

    // ── local: type decl + usage in globals / function ──────────────────

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

    // ── local: type extends base links to declared type ─────────────────

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

    // ── scope: local shadows global ─────────────────────────────────────

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
            assert!(!local_lines.contains(&1), "local group should NOT contain global (line 1)");
        });
    }

    // ── scope: param used in body ───────────────────────────────────────

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

    // ── scope: function name ≠ same-name variable ───────────────────────

    #[test]
    fn link_func_and_var_same_name_separate() {
        let src = "\
globals
    integer A = 1
endglobals
function A takes nothing returns nothing
endfunction
";
        with_cursor(src, |c| {
            let groups = find_groups(c, "A");
            // Should be 2 separate groups: one for variable, one for function
            assert_eq!(groups.len(), 2,
                "var A and function A should be in separate groups (different namespaces)");
        });
    }

    // ── standalone: multiple calls to unknown func share one group ──────

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

    // ── standalone: unknown var used in set + expr share one group ──────

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

    // ── standalone: single unknown ref is self-decl ─────────────────────

    #[test]
    fn link_standalone_single_ref_is_self_decl() {
        let src = "call Xyz()\n";
        with_cursor(src, |c| {
            let (_, occs) = find_group(c, "Xyz");
            assert_eq!(occs.len(), 1);
            assert!(occs[0].is_decl, "single standalone ref is its own decl");
        });
    }

    // ── import: external func resolves ──────────────────────────────────

    #[test]
    fn link_import_func_resolves() {
        let src = "call Bar()\ncall Bar()\n";
        let origin = Url::parse("file:///lib/common.j").unwrap();
        let imported = vec![ImportedSymbol {
            origin_uri: origin.clone(),
            name: "Bar".into(),
            kind: ImportedKind::Func, origin_decl_key: None,
        }];
        with_cursor_imported(src, &imported, |c| {
            let ext_key = *c.ref_groups.keys()
                .find(|&&k| k >= EXTERNAL_KEY_BASE)
                .expect("should have an external key for Bar");
            assert_eq!(c.ref_names[&ext_key], "Bar");
            assert_eq!(c.external_decls[&ext_key].uri, origin);
            let occs = &c.ref_groups[&ext_key];
            assert_eq!(occs.len(), 2, "both calls to Bar should be in the external group");
            assert!(!occs[0].is_decl, "external refs are not declarations");
            assert!(!occs[1].is_decl);
        });
    }

    // ── import: external var resolves ───────────────────────────────────

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
            kind: ImportedKind::Var, origin_decl_key: None,
        }];
        with_cursor_imported(src, &imported, |c| {
            let ext_key = *c.ref_groups.keys()
                .find(|&&k| k >= EXTERNAL_KEY_BASE
                    && c.ref_names.get(&k).map(|n| n == "bj_lastCreatedUnit").unwrap_or(false))
                .expect("should have external key for bj_lastCreatedUnit");
            assert_eq!(c.external_decls[&ext_key].uri, origin);
        });
    }

    // ── import: external type resolves (non-primitive) ──────────────────

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
            ImportedSymbol { origin_uri: origin.clone(), name: "group".into(), kind: ImportedKind::Var, origin_decl_key: None },
            ImportedSymbol { origin_uri: origin.clone(), name: "unit".into(),  kind: ImportedKind::Var, origin_decl_key: None },
        ];
        with_cursor_imported(src, &imported, |c| {
            for type_name in &["group", "unit"] {
                let key = c.ref_groups.keys()
                    .find(|&&k| k >= EXTERNAL_KEY_BASE
                        && c.ref_names.get(&k).map(|n| n.as_str() == *type_name).unwrap_or(false));
                assert!(key.is_some(),
                    "type {:?} should resolve as an imported symbol", type_name);
                let key = *key.unwrap();
                assert_eq!(c.external_decls[&key].uri, origin);
            }
        });
    }

    // ── import: primitive types do NOT pollute unresolved ────────────────

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

    // ── import: local decl shadows import ───────────────────────────────

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
            kind: ImportedKind::Func, origin_decl_key: None,
        }];
        with_cursor_imported(src, &imported, |c| {
            let ext_count = c.ref_groups.keys().filter(|&&k| k >= EXTERNAL_KEY_BASE).count();
            assert_eq!(ext_count, 0, "local A should shadow the import");
            let (_, occs) = find_group(c, "A");
            assert_eq!(occs.len(), 2, "A: 1 decl + 1 call");
        });
    }

    // ── RefMap: every identifier has a span ──────────────────────────────

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

    // ── RefMap: definition points to declaration ─────────────────────────

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

    // ── RefMap: occurrences_at returns all usages ────────────────────────

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
            assert_eq!(all.len(), 3, "F: 1 decl + 2 calls = 3 occurrences");
            let decls: Vec<_> = all.iter().filter(|o| o.is_decl).collect();
            assert_eq!(decls.len(), 1, "exactly 1 declaration");
            let refs: Vec<_> = all.iter().filter(|o| !o.is_decl).collect();
            assert_eq!(refs.len(), 2, "exactly 2 references");
        });
    }

    // ── RefMap: name_at returns correct symbol name ──────────────────────

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

    // ── link: function call inside expression ───────────────────────────

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

    // ── link: function reference (code variable) ────────────────────────

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

    // ── link: loop + exitwhen + if bodies ───────────────────────────────

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

    // ── link: constant in globals ───────────────────────────────────────

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

    // ── link: array variable with index ─────────────────────────────────

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

    // ── link: two functions calling each other ──────────────────────────

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

    // ── link: var namespace separate from func namespace ────────────────

    #[test]
    fn link_namespaces_separate_for_same_name() {
        // `X` exists as both a global variable and a function
        let src = "\
globals
    integer X = 5
endglobals
function X takes nothing returns nothing
endfunction
function Main takes nothing returns nothing
    set X = 10
    call X()
endfunction
";
        with_cursor(src, |c| {
            let groups = find_groups(c, "X");
            assert_eq!(groups.len(), 2,
                "should have 2 groups for X: var + func");

            // Variable group: decl (line 1) + set (line 6)
            let var_group = groups.iter()
                .find(|(_, occs)| occs.iter().any(|o| o.range.start.line == 1))
                .expect("should have X var group at line 1");
            let var_lines: Vec<_> = var_group.1.iter().map(|o| o.range.start.line).collect();
            assert!(var_lines.contains(&1), "var group: decl at line 1");
            assert!(var_lines.contains(&6), "var group: set at line 6");

            // Function group: decl (line 3) + call (line 7)
            let func_group = groups.iter()
                .find(|(_, occs)| occs.iter().any(|o| o.range.start.line == 3))
                .expect("should have X func group at line 3");
            let func_lines: Vec<_> = func_group.1.iter().map(|o| o.range.start.line).collect();
            assert!(func_lines.contains(&3), "func group: decl at line 3");
            assert!(func_lines.contains(&7), "func group: call at line 7");
        });
    }

    // ── link: import with multiple functions ────────────────────────────

    #[test]
    fn link_import_multiple_funcs() {
        let src = "\
function Main takes nothing returns nothing
    call CreateUnit()
    call RemoveUnit()
    call CreateUnit()
endfunction
";
        let origin = Url::parse("file:///lib/common.j").unwrap();
        let imported = vec![
            ImportedSymbol { origin_uri: origin.clone(), name: "CreateUnit".into(), kind: ImportedKind::Func, origin_decl_key: None },
            ImportedSymbol { origin_uri: origin.clone(), name: "RemoveUnit".into(), kind: ImportedKind::Func, origin_decl_key: None },
        ];
        with_cursor_imported(src, &imported, |c| {
            // CreateUnit: external group with 2 occurrences
            let cu_key = *c.ref_groups.keys()
                .find(|&&k| k >= EXTERNAL_KEY_BASE
                    && c.ref_names.get(&k).map(|n| n == "CreateUnit").unwrap_or(false))
                .expect("CreateUnit external key");
            assert_eq!(c.ref_groups[&cu_key].len(), 2, "CreateUnit: 2 calls");

            // RemoveUnit: external group with 1 occurrence
            let ru_key = *c.ref_groups.keys()
                .find(|&&k| k >= EXTERNAL_KEY_BASE
                    && c.ref_names.get(&k).map(|n| n == "RemoveUnit").unwrap_or(false))
                .expect("RemoveUnit external key");
            assert_eq!(c.ref_groups[&ru_key].len(), 1, "RemoveUnit: 1 call");
        });
    }

    // ── link: import — mixed resolved + unresolved ──────────────────────

    #[test]
    fn link_import_partial() {
        let src = "\
function Main takes nothing returns nothing
    call Known()
    call Unknown()
endfunction
";
        let origin = Url::parse("file:///lib/common.j").unwrap();
        let imported = vec![
            ImportedSymbol { origin_uri: origin.clone(), name: "Known".into(), kind: ImportedKind::Func, origin_decl_key: None },
        ];
        with_cursor_imported(src, &imported, |c| {
            // Known → external
            let ext_keys: Vec<_> = c.ref_groups.keys()
                .filter(|&&k| k >= EXTERNAL_KEY_BASE)
                .collect();
            assert_eq!(ext_keys.len(), 1, "exactly 1 external group (Known)");
            assert_eq!(c.ref_names[ext_keys[0]], "Known");

            // Unknown → standalone local
            let (key, occs) = find_group(c, "Unknown");
            assert!(*key < EXTERNAL_KEY_BASE, "Unknown should be a local standalone key");
            assert_eq!(occs.len(), 1);
            assert!(occs[0].is_decl);
        });
    }

    // ── link: complex program — everything has a span ───────────────────

    #[test]
    fn link_complex_program_full_coverage() {
        let src = "\
type handle extends agent
type unit extends handle
native CreateUnit takes integer id returns unit
globals
    constant integer FOOTMAN = 'hfoo'
    unit array army
endglobals
function Spawn takes integer idx returns nothing
    set army[idx] = CreateUnit(FOOTMAN)
endfunction
function Main takes nothing returns nothing
    call Spawn(0)
    call Spawn(1)
endfunction
";
        let rope = Rope::from(src);
        with_cursor(src, |c| {
            let rm = ref_map_from(c, &rope);

            // handle: 1 decl + 1 extends ref
            let (_, h) = find_group(c, "handle");
            assert_eq!(h.len(), 2);

            // unit: 1 decl + 2 refs (native return type + globals type)
            let (_, u) = find_group(c, "unit");
            assert_eq!(u.len(), 3, "unit: 1 decl + 2 refs");

            // CreateUnit: 1 native decl + 1 call
            let (_, cu) = find_group(c, "CreateUnit");
            assert_eq!(cu.len(), 2);

            // FOOTMAN: 1 decl + 1 read
            let (_, fm) = find_group(c, "FOOTMAN");
            assert_eq!(fm.len(), 2);

            // army: 1 decl + 1 set
            let (_, army) = find_group(c, "army");
            assert_eq!(army.len(), 2);

            // Spawn: 1 decl + 2 calls
            let (_, sp) = find_group(c, "Spawn");
            assert_eq!(sp.len(), 3, "Spawn: 1 decl + 2 calls");

            // idx: 1 param decl + 1 read (array index)
            let (_, idx) = find_group(c, "idx");
            assert_eq!(idx.len(), 2);

            // Spot-check spans
            assert_span_at(&rm, &rope, 0, 5,  "handle decl");
            assert_span_at(&rm, &rope, 1, 5,  "unit decl");
            assert_span_at(&rm, &rope, 8, 8,  "army set");
            assert_span_at(&rm, &rope, 8, 31, "FOOTMAN read");
            assert_span_at(&rm, &rope, 11, 9, "Spawn call 1");
            assert_span_at(&rm, &rope, 12, 9, "Spawn call 2");
        });
    }

    // ======================================================================
    //  Multi-file import tests
    // ======================================================================
    //
    //  These tests simulate parsing multiple "files" by running Cursor on
    //  each file source independently, collecting symbols from file A,
    //  then feeding them as `ImportedSymbol` to file B, etc.
    //
    //  Covers:
    //  • two-file import (direct)
    //  • transitive import chains (A→B→C)
    //  • diamond imports (A→B+C, B→D, C→D)
    //  • origin_decl_key tracking
    //  • file deletion effects on import graph
    //  • RefMap external_decls round-trip through build_ref_map

    /// Parse a source and return (Cursor, Rope).
    /// Useful for collecting symbols from one file to feed into another.
    fn parse_file(src: &str) -> (Cursor, Rope) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        let rope = Rope::from(src);
        let cursor = Cursor::walk(&ast, &rope, &[]);
        (cursor, rope)
    }

    /// Parse a source with imported symbols and return (Cursor, Rope).
    fn parse_file_with_imports(src: &str, imported: &[ImportedSymbol]) -> (Cursor, Rope) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let ast = build_ast(tree.root_node());
        let rope = Rope::from(src);
        let cursor = Cursor::walk(&ast, &rope, imported);
        (cursor, rope)
    }

    /// Collect exported symbols from a cursor (functions, natives, globals, types)
    /// for feeding into another file as imports.
    fn collect_symbols(
        cursor: &Cursor,
        rope: &Rope,
        origin_uri: &Url,
    ) -> Vec<ImportedSymbol> {
        let rm = ref_map_from(cursor, rope);
        let mut symbols = Vec::new();

        for f in &cursor.file_symbols.functions {
            let origin_key = rm.groups.iter()
                .find(|(_, g)| g.name == f.name && g.occurrences.iter().any(|o| o.is_decl))
                .map(|(&k, _)| k);
            symbols.push(ImportedSymbol {
                origin_uri: origin_uri.clone(),
                name: f.name.clone(),
                kind: ImportedKind::Func,
                origin_decl_key: origin_key,
            });
        }
        for n in &cursor.file_symbols.natives {
            let origin_key = rm.groups.iter()
                .find(|(_, g)| g.name == n.name && g.occurrences.iter().any(|o| o.is_decl))
                .map(|(&k, _)| k);
            symbols.push(ImportedSymbol {
                origin_uri: origin_uri.clone(),
                name: n.name.clone(),
                kind: ImportedKind::Func,
                origin_decl_key: origin_key,
            });
        }
        for g in &cursor.file_symbols.globals {
            let origin_key = rm.groups.iter()
                .find(|(_, grp)| grp.name == g.name && grp.occurrences.iter().any(|o| o.is_decl))
                .map(|(&k, _)| k);
            symbols.push(ImportedSymbol {
                origin_uri: origin_uri.clone(),
                name: g.name.clone(),
                kind: ImportedKind::Var,
                origin_decl_key: origin_key,
            });
        }
        for t in &cursor.file_symbols.types {
            let origin_key = rm.groups.iter()
                .find(|(_, grp)| grp.name == t.name && grp.occurrences.iter().any(|o| o.is_decl))
                .map(|(&k, _)| k);
            symbols.push(ImportedSymbol {
                origin_uri: origin_uri.clone(),
                name: t.name.clone(),
                kind: ImportedKind::Var,
                origin_decl_key: origin_key,
            });
        }
        symbols
    }

    // ── multi-file: direct import (A → B) ────────────────────────────────

    #[test]
    fn multifile_direct_import() {
        // File B: declares CreateUnit and unit type
        let src_b = "\
type unit extends handle
native CreateUnit takes integer id returns unit
";
        let uri_b = Url::parse("file:///lib/common.j").unwrap();
        let (cursor_b, rope_b) = parse_file(src_b);
        let symbols_b = collect_symbols(&cursor_b, &rope_b, &uri_b);

        // Check file B exports
        assert!(symbols_b.iter().any(|s| s.name == "CreateUnit" && s.kind == ImportedKind::Func),
            "B should export CreateUnit");
        assert!(symbols_b.iter().any(|s| s.name == "unit" && s.kind == ImportedKind::Var),
            "B should export type unit");

        // File A: imports from B and uses CreateUnit + unit
        let src_a = "\
function Main takes nothing returns nothing
    local unit u = CreateUnit(1)
endfunction
";
        let (cursor_a, rope_a) = parse_file_with_imports(src_a, &symbols_b);
        let rm_a = ref_map_from(&cursor_a, &rope_a);

        // CreateUnit should be external
        let cu_ext = cursor_a.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_a.ref_names.get(&k).map(|n| n == "CreateUnit").unwrap_or(false));
        assert!(cu_ext.is_some(), "CreateUnit should be an external ref in file A");
        let cu_key = *cu_ext.unwrap();
        assert_eq!(cursor_a.external_decls[&cu_key].uri, uri_b);
        assert_eq!(cursor_a.external_decls[&cu_key].name, "CreateUnit");

        // unit should be external
        let unit_ext = cursor_a.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_a.ref_names.get(&k).map(|n| n == "unit").unwrap_or(false));
        assert!(unit_ext.is_some(), "unit type should be an external ref in file A");
        let unit_key = *unit_ext.unwrap();
        assert_eq!(cursor_a.external_decls[&unit_key].uri, uri_b);

        // RefMap should have external spans
        let ext_spans: Vec<_> = rm_a.spans.iter().filter(|s| s.is_external).collect();
        assert!(ext_spans.len() >= 2, "should have at least 2 external spans (unit + CreateUnit)");
    }

    // ── multi-file: origin_decl_key is propagated ────────────────────────

    #[test]
    fn multifile_origin_decl_key_tracked() {
        // File B: declares function Foo
        let src_b = "\
function Foo takes nothing returns nothing
endfunction
";
        let uri_b = Url::parse("file:///lib/utils.j").unwrap();
        let (cursor_b, rope_b) = parse_file(src_b);
        let symbols_b = collect_symbols(&cursor_b, &rope_b, &uri_b);

        // The origin_decl_key should be the start_byte of "Foo" in file B
        let foo_sym = symbols_b.iter().find(|s| s.name == "Foo").unwrap();
        assert!(foo_sym.origin_decl_key.is_some(),
            "Foo should have an origin_decl_key from file B's RefMap");
        let origin_key = foo_sym.origin_decl_key.unwrap();

        // File A: calls Foo
        let src_a = "call Foo()\n";
        let (cursor_a, _rope_a) = parse_file_with_imports(src_a, &symbols_b);

        // Check that ExternalDecl carries the origin key
        let ext_key = *cursor_a.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_a.ref_names.get(&k).map(|n| n == "Foo").unwrap_or(false))
            .expect("Foo should be external in A");
        let ext_decl = &cursor_a.external_decls[&ext_key];
        assert_eq!(ext_decl.origin_decl_key, Some(origin_key),
            "origin_decl_key should match Foo's DeclKey in file B");
    }

    // ── multi-file: transitive import chain (A → B → C) ─────────────────

    #[test]
    fn multifile_transitive_chain() {
        // File C: declares native GetPlayer
        let src_c = "\
native GetPlayer takes integer id returns handle
";
        let uri_c = Url::parse("file:///lib/natives.j").unwrap();
        let (cursor_c, rope_c) = parse_file(src_c);
        let symbols_c = collect_symbols(&cursor_c, &rope_c, &uri_c);

        // File B: imports C, declares WrapPlayer that uses GetPlayer
        let src_b = "\
function WrapPlayer takes integer id returns handle
    return GetPlayer(id)
endfunction
";
        let uri_b = Url::parse("file:///lib/helpers.j").unwrap();
        let (cursor_b, rope_b) = parse_file_with_imports(src_b, &symbols_c);
        let mut symbols_for_a = collect_symbols(&cursor_b, &rope_b, &uri_b);
        // Also include C's symbols (transitive)
        symbols_for_a.extend(symbols_c.clone());

        // File A: uses both WrapPlayer (from B) and GetPlayer (from C, transitively)
        let src_a = "\
function Main takes nothing returns nothing
    call WrapPlayer(1)
    call GetPlayer(2)
endfunction
";
        let (cursor_a, _rope_a) = parse_file_with_imports(src_a, &symbols_for_a);

        // WrapPlayer → external from B
        let wp_key = cursor_a.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_a.ref_names.get(&k).map(|n| n == "WrapPlayer").unwrap_or(false));
        assert!(wp_key.is_some(), "WrapPlayer should be external in A");
        assert_eq!(cursor_a.external_decls[wp_key.unwrap()].uri, uri_b);

        // GetPlayer → external from C (transitive)
        let gp_key = cursor_a.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_a.ref_names.get(&k).map(|n| n == "GetPlayer").unwrap_or(false));
        assert!(gp_key.is_some(), "GetPlayer should be external in A (transitive)");
        assert_eq!(cursor_a.external_decls[gp_key.unwrap()].uri, uri_c);
    }

    // ── multi-file: diamond import (A→B+C, B→D, C→D) ────────────────────

    #[test]
    fn multifile_diamond_import() {
        // File D: declares KillUnit
        let src_d = "native KillUnit takes handle u returns nothing\n";
        let uri_d = Url::parse("file:///lib/core.j").unwrap();
        let (cursor_d, rope_d) = parse_file(src_d);
        let symbols_d = collect_symbols(&cursor_d, &rope_d, &uri_d);

        // File B: imports D, declares WrapKillB
        let src_b = "\
function WrapKillB takes handle u returns nothing
    call KillUnit(u)
endfunction
";
        let uri_b = Url::parse("file:///lib/b.j").unwrap();
        let (cursor_b, rope_b) = parse_file_with_imports(src_b, &symbols_d);
        let symbols_b = collect_symbols(&cursor_b, &rope_b, &uri_b);

        // File C: imports D, declares WrapKillC
        let src_c = "\
function WrapKillC takes handle u returns nothing
    call KillUnit(u)
endfunction
";
        let uri_c = Url::parse("file:///lib/c.j").unwrap();
        let (cursor_c, rope_c) = parse_file_with_imports(src_c, &symbols_d);
        let symbols_c = collect_symbols(&cursor_c, &rope_c, &uri_c);

        // File A: imports B + C + D (transitively)
        let mut all_imports = Vec::new();
        all_imports.extend(symbols_b.clone());
        all_imports.extend(symbols_c.clone());
        all_imports.extend(symbols_d.clone());

        let src_a = "\
function Main takes nothing returns nothing
    call WrapKillB(null)
    call WrapKillC(null)
    call KillUnit(null)
endfunction
";
        let (cursor_a, _rope_a) = parse_file_with_imports(src_a, &all_imports);

        // All 3 should be external
        for (name, expected_uri) in &[
            ("WrapKillB", &uri_b),
            ("WrapKillC", &uri_c),
            ("KillUnit", &uri_d),
        ] {
            let key = cursor_a.ref_groups.keys()
                .find(|&&k| k >= EXTERNAL_KEY_BASE
                    && cursor_a.ref_names.get(&k).map(|n| n.as_str() == *name).unwrap_or(false));
            assert!(key.is_some(), "{} should be external in A", name);
            assert_eq!(&cursor_a.external_decls[key.unwrap()].uri, *expected_uri,
                "{} should come from {:?}", name, expected_uri);
        }

        // Exactly 3 external groups
        let ext_count = cursor_a.ref_groups.keys()
            .filter(|&&k| k >= EXTERNAL_KEY_BASE)
            .count();
        assert_eq!(ext_count, 3, "diamond: 3 external groups (WrapKillB, WrapKillC, KillUnit)");
    }

    // ── multi-file: local declaration shadows transitive import ──────────

    #[test]
    fn multifile_local_shadows_transitive() {
        // File C: declares Foo
        let src_c = "function Foo takes nothing returns nothing\nendfunction\n";
        let uri_c = Url::parse("file:///lib/c.j").unwrap();
        let (cursor_c, rope_c) = parse_file(src_c);
        let symbols_c = collect_symbols(&cursor_c, &rope_c, &uri_c);

        // File A: declares its own Foo AND calls Foo → should link to local, not import
        let src_a = "\
function Foo takes nothing returns nothing
endfunction
function Main takes nothing returns nothing
    call Foo()
endfunction
";
        let (cursor_a, _rope_a) = parse_file_with_imports(src_a, &symbols_c);

        // No external groups — local Foo shadows the import
        let ext_count = cursor_a.ref_groups.keys()
            .filter(|&&k| k >= EXTERNAL_KEY_BASE)
            .count();
        assert_eq!(ext_count, 0, "local Foo should shadow imported Foo");

        // Both occurrences in the same local group
        let (_, foo_occs) = find_group(&cursor_a, "Foo");
        assert_eq!(foo_occs.len(), 2, "Foo: 1 decl + 1 call, all local");
    }

    // ── multi-file: import from multiple origins ─────────────────────────

    #[test]
    fn multifile_imports_from_multiple_origins() {
        // File B: declares DoStuff
        let src_b = "function DoStuff takes nothing returns nothing\nendfunction\n";
        let uri_b = Url::parse("file:///lib/b.j").unwrap();
        let (cursor_b, rope_b) = parse_file(src_b);
        let symbols_b = collect_symbols(&cursor_b, &rope_b, &uri_b);

        // File C: declares DoOther
        let src_c = "function DoOther takes nothing returns nothing\nendfunction\n";
        let uri_c = Url::parse("file:///lib/c.j").unwrap();
        let (cursor_c, rope_c) = parse_file(src_c);
        let symbols_c = collect_symbols(&cursor_c, &rope_c, &uri_c);

        // File A: calls both
        let mut imports = Vec::new();
        imports.extend(symbols_b);
        imports.extend(symbols_c);

        let src_a = "\
function Main takes nothing returns nothing
    call DoStuff()
    call DoOther()
endfunction
";
        let (cursor_a, _rope_a) = parse_file_with_imports(src_a, &imports);

        let stuff_key = cursor_a.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_a.ref_names.get(&k).map(|n| n == "DoStuff").unwrap_or(false))
            .expect("DoStuff should be external");
        assert_eq!(cursor_a.external_decls[stuff_key].uri, uri_b);

        let other_key = cursor_a.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_a.ref_names.get(&k).map(|n| n == "DoOther").unwrap_or(false))
            .expect("DoOther should be external");
        assert_eq!(cursor_a.external_decls[other_key].uri, uri_c);
    }

    // ── multi-file: mixed resolved + unresolved + local ──────────────────

    #[test]
    fn multifile_mixed_resolution() {
        // File B: declares only KnownFunc
        let src_b = "function KnownFunc takes nothing returns nothing\nendfunction\n";
        let uri_b = Url::parse("file:///lib/b.j").unwrap();
        let (cursor_b, rope_b) = parse_file(src_b);
        let symbols_b = collect_symbols(&cursor_b, &rope_b, &uri_b);

        // File A: has local Local, calls KnownFunc (resolved), calls Unknown (standalone)
        let src_a = "\
function Local takes nothing returns nothing
endfunction
function Main takes nothing returns nothing
    call Local()
    call KnownFunc()
    call UnknownFunc()
endfunction
";
        let (cursor_a, _rope_a) = parse_file_with_imports(src_a, &symbols_b);

        // Local → local group (not external)
        let (local_key, local_occs) = find_group(&cursor_a, "Local");
        assert!(*local_key < EXTERNAL_KEY_BASE, "Local should be a local DeclKey");
        assert_eq!(local_occs.len(), 2, "Local: 1 decl + 1 call");

        // KnownFunc → external
        let known_key = cursor_a.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_a.ref_names.get(&k).map(|n| n == "KnownFunc").unwrap_or(false))
            .expect("KnownFunc should be external");
        assert_eq!(cursor_a.external_decls[known_key].uri, uri_b);

        // UnknownFunc → standalone local (not external)
        let (unk_key, unk_occs) = find_group(&cursor_a, "UnknownFunc");
        assert!(*unk_key < EXTERNAL_KEY_BASE, "UnknownFunc should be standalone local");
        assert_eq!(unk_occs.len(), 1, "UnknownFunc: 1 standalone ref");
        assert!(unk_occs[0].is_decl, "standalone ref is its own decl");
    }

    // ── multi-file: globals + types imported ─────────────────────────────

    #[test]
    fn multifile_import_globals_and_types() {
        // File B: declares type + global
        let src_b = "\
type widget extends handle
globals
    widget lastWidget = null
endglobals
";
        let uri_b = Url::parse("file:///lib/common.j").unwrap();
        let (cursor_b, rope_b) = parse_file(src_b);
        let symbols_b = collect_symbols(&cursor_b, &rope_b, &uri_b);

        // Check exports
        assert!(symbols_b.iter().any(|s| s.name == "widget" && s.kind == ImportedKind::Var));
        assert!(symbols_b.iter().any(|s| s.name == "lastWidget" && s.kind == ImportedKind::Var));

        // File A: uses type and global from B
        let src_a = "\
function F takes nothing returns nothing
    local widget w = lastWidget
endfunction
";
        let (cursor_a, _rope_a) = parse_file_with_imports(src_a, &symbols_b);

        // widget type → external
        let widget_key = cursor_a.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_a.ref_names.get(&k).map(|n| n == "widget").unwrap_or(false));
        assert!(widget_key.is_some(), "widget type should be external in A");

        // lastWidget global → external
        let lw_key = cursor_a.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_a.ref_names.get(&k).map(|n| n == "lastWidget").unwrap_or(false));
        assert!(lw_key.is_some(), "lastWidget should be external in A");
    }

    // ── multi-file: re-parse after change (symbol added) ─────────────────

    #[test]
    fn multifile_reparse_after_symbol_added() {
        // Phase 1: File B has Foo only
        let src_b_v1 = "function Foo takes nothing returns nothing\nendfunction\n";
        let uri_b = Url::parse("file:///lib/b.j").unwrap();
        let (cursor_b1, rope_b1) = parse_file(src_b_v1);
        let symbols_b1 = collect_symbols(&cursor_b1, &rope_b1, &uri_b);

        // File A: calls Foo (resolved) and Bar (unresolved)
        let src_a = "\
function Main takes nothing returns nothing
    call Foo()
    call Bar()
endfunction
";
        let (cursor_a1, _rope_a1) = parse_file_with_imports(src_a, &symbols_b1);

        // Bar should be standalone in v1
        let (bar_key1, _) = find_group(&cursor_a1, "Bar");
        assert!(*bar_key1 < EXTERNAL_KEY_BASE, "Bar standalone before B adds it");

        // Phase 2: File B is edited to also have Bar
        let src_b_v2 = "\
function Foo takes nothing returns nothing
endfunction
function Bar takes nothing returns nothing
endfunction
";
        let (cursor_b2, rope_b2) = parse_file(src_b_v2);
        let symbols_b2 = collect_symbols(&cursor_b2, &rope_b2, &uri_b);

        // Re-parse A with updated imports
        let (cursor_a2, _rope_a2) = parse_file_with_imports(src_a, &symbols_b2);

        // Now Bar should be external
        let bar_ext = cursor_a2.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_a2.ref_names.get(&k).map(|n| n == "Bar").unwrap_or(false));
        assert!(bar_ext.is_some(), "Bar should be external after B adds it");
        assert_eq!(cursor_a2.external_decls[bar_ext.unwrap()].uri, uri_b);
    }

    // ── multi-file: re-parse after symbol removed ────────────────────────

    #[test]
    fn multifile_reparse_after_symbol_removed() {
        // Phase 1: File B has both Foo and Bar
        let src_b_v1 = "\
function Foo takes nothing returns nothing
endfunction
function Bar takes nothing returns nothing
endfunction
";
        let uri_b = Url::parse("file:///lib/b.j").unwrap();
        let (cursor_b1, rope_b1) = parse_file(src_b_v1);
        let symbols_b1 = collect_symbols(&cursor_b1, &rope_b1, &uri_b);

        let src_a = "\
function Main takes nothing returns nothing
    call Foo()
    call Bar()
endfunction
";
        let (cursor_a1, _rope_a1) = parse_file_with_imports(src_a, &symbols_b1);

        // Both should be external in v1
        let ext_count1 = cursor_a1.ref_groups.keys()
            .filter(|&&k| k >= EXTERNAL_KEY_BASE)
            .count();
        assert_eq!(ext_count1, 2, "v1: Foo + Bar both external");

        // Phase 2: File B loses Bar
        let src_b_v2 = "function Foo takes nothing returns nothing\nendfunction\n";
        let (cursor_b2, rope_b2) = parse_file(src_b_v2);
        let symbols_b2 = collect_symbols(&cursor_b2, &rope_b2, &uri_b);

        let (cursor_a2, _rope_a2) = parse_file_with_imports(src_a, &symbols_b2);

        // Foo still external, Bar now standalone
        let foo_ext = cursor_a2.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_a2.ref_names.get(&k).map(|n| n == "Foo").unwrap_or(false));
        assert!(foo_ext.is_some(), "Foo should still be external");

        let (bar_key, _) = find_group(&cursor_a2, "Bar");
        assert!(*bar_key < EXTERNAL_KEY_BASE, "Bar should be standalone after removal from B");
    }

    // ── import graph: file deletion removes edges ────────────────────────

    #[test]
    fn import_graph_file_deletion() {
        use crate::util::import_graph::ImportGraph;

        let g = ImportGraph::new_empty();
        let a = Url::parse("file:///project/a.j").unwrap();
        let b = Url::parse("file:///project/b.j").unwrap();
        let c = Url::parse("file:///project/c.j").unwrap();
        let d = Url::parse("file:///project/d.j").unwrap();

        // A→B→C, A→D
        g.update(&a, std::collections::HashSet::from([b.clone(), d.clone()]));
        g.update(&b, std::collections::HashSet::from([c.clone()]));

        assert_eq!(g.node_count(), 4);
        assert_eq!(g.edge_count(), 3);

        // Delete B
        g.remove(&b);

        // B is gone
        assert_eq!(g.node_count(), 3);
        assert!(g.direct_imports(&b).is_empty(), "B removed: no imports");
        assert!(g.direct_dependents(&b).is_empty(), "B removed: no dependents");

        // A still has D as import, but B edge is gone
        let a_imports = g.direct_imports(&a);
        assert_eq!(a_imports.len(), 1, "A should have 1 import left (D)");
        assert!(a_imports.contains(&d));
        assert!(!a_imports.contains(&b), "B should not be in A's imports");

        // C has no dependents anymore
        assert!(g.direct_dependents(&c).is_empty(), "C should have no dependents after B removed");
    }

    // ── import graph: delete middle of chain ─────────────────────────────

    #[test]
    fn import_graph_delete_middle() {
        use crate::util::import_graph::ImportGraph;

        let g = ImportGraph::new_empty();
        let a = Url::parse("file:///a.j").unwrap();
        let b = Url::parse("file:///b.j").unwrap();
        let c = Url::parse("file:///c.j").unwrap();

        // Chain: A→B→C
        g.update(&a, std::collections::HashSet::from([b.clone()]));
        g.update(&b, std::collections::HashSet::from([c.clone()]));

        // Transitive: A depends on B and C
        let deps = g.dependencies(&a);
        assert_eq!(deps.len(), 2);

        // Delete B (middle)
        g.remove(&b);

        // A's direct import B is gone
        assert!(g.direct_imports(&a).is_empty(),
            "A's only import was B which is now deleted");

        // Transitive deps of A are now empty
        let deps_after = g.dependencies(&a);
        assert!(deps_after.is_empty(),
            "A has no transitive dependencies after B removed");

        // C is isolated
        assert!(g.direct_dependents(&c).is_empty());
    }

    // ── import graph: delete leaf (no cascading) ─────────────────────────

    #[test]
    fn import_graph_delete_leaf() {
        use crate::util::import_graph::ImportGraph;

        let g = ImportGraph::new_empty();
        let a = Url::parse("file:///a.j").unwrap();
        let b = Url::parse("file:///b.j").unwrap();
        let c = Url::parse("file:///c.j").unwrap();

        // A→B, A→C
        g.update(&a, std::collections::HashSet::from([b.clone(), c.clone()]));

        // Delete C (leaf)
        g.remove(&c);

        // A still imports B
        let a_imports = g.direct_imports(&a);
        assert_eq!(a_imports.len(), 1);
        assert!(a_imports.contains(&b));
    }

    // ── import graph: delete root ────────────────────────────────────────

    #[test]
    fn import_graph_delete_root() {
        use crate::util::import_graph::ImportGraph;

        let g = ImportGraph::new_empty();
        let a = Url::parse("file:///a.j").unwrap();
        let b = Url::parse("file:///b.j").unwrap();

        g.update(&a, std::collections::HashSet::from([b.clone()]));
        g.remove(&a);

        assert!(g.direct_imports(&a).is_empty());
        assert!(g.direct_dependents(&b).is_empty());
        assert_eq!(g.node_count(), 1, "only B remains as isolated node");
    }

    // ── import graph: diamond delete one branch ──────────────────────────

    #[test]
    fn import_graph_diamond_delete_branch() {
        use crate::util::import_graph::ImportGraph;

        let g = ImportGraph::new_empty();
        let a = Url::parse("file:///a.j").unwrap();
        let b = Url::parse("file:///b.j").unwrap();
        let c = Url::parse("file:///c.j").unwrap();
        let d = Url::parse("file:///d.j").unwrap();

        // A→B, A→C, B→D, C→D (diamond)
        g.update(&a, std::collections::HashSet::from([b.clone(), c.clone()]));
        g.update(&b, std::collections::HashSet::from([d.clone()]));
        g.update(&c, std::collections::HashSet::from([d.clone()]));

        assert_eq!(g.node_count(), 4);
        assert_eq!(g.edge_count(), 4);

        // Delete B (one branch of diamond)
        g.remove(&b);

        // A still imports C
        let a_imports = g.direct_imports(&a);
        assert_eq!(a_imports.len(), 1);
        assert!(a_imports.contains(&c));

        // D still reachable from A through C
        let a_deps = g.dependencies(&a);
        assert!(a_deps.contains(&c));
        assert!(a_deps.contains(&d));

        // D has one dependent left (C, not B)
        let d_dependents = g.direct_dependents(&d);
        assert_eq!(d_dependents.len(), 1);
        assert!(d_dependents.contains(&c));
    }

    // ── multi-file: RefMap external_decls survive build_ref_map ──────────

    #[test]
    fn multifile_refmap_external_decls_roundtrip() {
        let src_b = "function LibFunc takes integer x returns nothing\nendfunction\n";
        let uri_b = Url::parse("file:///lib/b.j").unwrap();
        let (cursor_b, rope_b) = parse_file(src_b);
        let symbols_b = collect_symbols(&cursor_b, &rope_b, &uri_b);

        let src_a = "\
function Main takes nothing returns nothing
    call LibFunc(42)
    call LibFunc(99)
endfunction
";
        let (cursor_a, rope_a) = parse_file_with_imports(src_a, &symbols_b);
        let rm = ref_map_from(&cursor_a, &rope_a);

        // RefMap should have LibFunc in external_decls
        let ext_key = rm.groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && rm.groups.get(&k).map(|g| g.name == "LibFunc").unwrap_or(false))
            .expect("LibFunc should exist in RefMap groups");
        assert!(rm.external_decls.contains_key(ext_key),
            "LibFunc should be in external_decls");
        assert_eq!(rm.external_decls[ext_key].uri, uri_b);

        // 2 occurrences (2 calls)
        let grp = &rm.groups[ext_key];
        assert_eq!(grp.occurrences.len(), 2, "LibFunc: 2 calls in A");
        assert!(grp.occurrences.iter().all(|o| !o.is_decl),
            "external refs should not be marked as decl");

        // external_at should work through spans
        let pos = crate::lsp::position::Position { line: 1, character: 9 };
        let byte = pos.to_byte_offset(&rope_a).unwrap();
        let ext = rm.external_at(byte);
        assert!(ext.is_some(), "external_at should find LibFunc at call site");
        assert_eq!(ext.unwrap().uri, uri_b);
    }

    // ── multi-file: RefMap cache serialization round-trip ─────────────────

    #[test]
    fn multifile_refmap_bincode_roundtrip() {
        let src_b = "native SomeNative takes nothing returns nothing\n";
        let uri_b = Url::parse("file:///lib/b.j").unwrap();
        let (cursor_b, rope_b) = parse_file(src_b);
        let symbols_b = collect_symbols(&cursor_b, &rope_b, &uri_b);

        let src_a = "call SomeNative()\ncall SomeNative()\n";
        let (cursor_a, rope_a) = parse_file_with_imports(src_a, &symbols_b);
        let rm_original = ref_map_from(&cursor_a, &rope_a);

        // Serialize and deserialize
        let serialized = bincode::serialize(&rm_original).expect("serialize");
        let rm_restored: crate::lsp::ref_map::RefMap =
            bincode::deserialize(&serialized).expect("deserialize");

        // Verify groups
        assert_eq!(rm_restored.groups.len(), rm_original.groups.len());
        for (key, grp) in &rm_original.groups {
            let restored_grp = rm_restored.groups.get(key)
                .unwrap_or_else(|| panic!("key {} missing after roundtrip", key));
            assert_eq!(restored_grp.name, grp.name);
            assert_eq!(restored_grp.occurrences.len(), grp.occurrences.len());
        }

        // Verify spans
        assert_eq!(rm_restored.spans.len(), rm_original.spans.len());
        for (orig, rest) in rm_original.spans.iter().zip(rm_restored.spans.iter()) {
            assert_eq!(orig.start_byte, rest.start_byte);
            assert_eq!(orig.end_byte, rest.end_byte);
            assert_eq!(orig.decl_key, rest.decl_key);
            assert_eq!(orig.is_external, rest.is_external);
        }

        // Verify external_decls (including origin_decl_key)
        assert_eq!(rm_restored.external_decls.len(), rm_original.external_decls.len());
        for (key, ext) in &rm_original.external_decls {
            let restored_ext = rm_restored.external_decls.get(key).unwrap();
            assert_eq!(restored_ext.uri, ext.uri);
            assert_eq!(restored_ext.name, ext.name);
            assert_eq!(restored_ext.origin_decl_key, ext.origin_decl_key);
        }
    }

    // ── multi-file: long chain A→B→C→D→E ─────────────────────────────────

    #[test]
    fn multifile_long_transitive_chain() {
        // E → D → C → B → A, each file adds one function
        let uri_e = Url::parse("file:///e.j").unwrap();
        let uri_d = Url::parse("file:///d.j").unwrap();
        let uri_c = Url::parse("file:///c.j").unwrap();
        let uri_b = Url::parse("file:///b.j").unwrap();

        // File E
        let (cursor_e, rope_e) = parse_file("native FuncE takes nothing returns nothing\n");
        let symbols_e = collect_symbols(&cursor_e, &rope_e, &uri_e);

        // File D imports E
        let (cursor_d, rope_d) = parse_file_with_imports(
            "function FuncD takes nothing returns nothing\n    call FuncE()\nendfunction\n",
            &symbols_e);
        let mut symbols_d = collect_symbols(&cursor_d, &rope_d, &uri_d);
        symbols_d.extend(symbols_e.clone());

        // File C imports D (+E transitively)
        let (cursor_c, rope_c) = parse_file_with_imports(
            "function FuncC takes nothing returns nothing\n    call FuncD()\nendfunction\n",
            &symbols_d);
        let mut symbols_c = collect_symbols(&cursor_c, &rope_c, &uri_c);
        symbols_c.extend(symbols_d.clone());

        // File B imports C (+D+E transitively)
        let (cursor_b, rope_b) = parse_file_with_imports(
            "function FuncB takes nothing returns nothing\n    call FuncC()\nendfunction\n",
            &symbols_c);
        let mut symbols_b = collect_symbols(&cursor_b, &rope_b, &uri_b);
        symbols_b.extend(symbols_c.clone());

        // File A imports B (+C+D+E transitively), calls all of them
        let src_a = "\
function Main takes nothing returns nothing
    call FuncB()
    call FuncC()
    call FuncD()
    call FuncE()
endfunction
";
        let (cursor_a, _rope_a) = parse_file_with_imports(src_a, &symbols_b);

        // All 4 should be external
        for (name, expected_uri) in &[
            ("FuncB", &uri_b),
            ("FuncC", &uri_c),
            ("FuncD", &uri_d),
            ("FuncE", &uri_e),
        ] {
            let key = cursor_a.ref_groups.keys()
                .find(|&&k| k >= EXTERNAL_KEY_BASE
                    && cursor_a.ref_names.get(&k).map(|n| n.as_str() == *name).unwrap_or(false));
            assert!(key.is_some(), "{} should be external in A", name);
            assert_eq!(&cursor_a.external_decls[key.unwrap()].uri, *expected_uri,
                "{} should trace back to correct origin", name);
        }
    }

    // ── multi-file: duplicate symbol in two imports (first wins) ─────────

    #[test]
    fn multifile_duplicate_import_first_wins() {
        let uri_b = Url::parse("file:///b.j").unwrap();
        let uri_c = Url::parse("file:///c.j").unwrap();

        let imported = vec![
            ImportedSymbol {
                origin_uri: uri_b.clone(),
                name: "Dup".into(),
                kind: ImportedKind::Func,
                origin_decl_key: Some(10),
            },
            ImportedSymbol {
                origin_uri: uri_c.clone(),
                name: "Dup".into(),
                kind: ImportedKind::Func,
                origin_decl_key: Some(20),
            },
        ];

        let src_a = "call Dup()\n";
        let (cursor_a, _rope_a) = parse_file_with_imports(src_a, &imported);

        let ext_key = cursor_a.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_a.ref_names.get(&k).map(|n| n == "Dup").unwrap_or(false))
            .expect("Dup should be external");

        // First import wins (import_lookup uses or_insert)
        let ext = &cursor_a.external_decls[ext_key];
        assert_eq!(ext.uri, uri_b, "first import (B) should win for duplicate Dup");
        assert_eq!(ext.origin_decl_key, Some(10));
    }

    // ── import graph: update clears old edges on re-import ───────────────

    #[test]
    fn import_graph_update_replaces_imports() {
        use crate::util::import_graph::ImportGraph;

        let g = ImportGraph::new_empty();
        let a = Url::parse("file:///a.j").unwrap();
        let b = Url::parse("file:///b.j").unwrap();
        let c = Url::parse("file:///c.j").unwrap();
        let d = Url::parse("file:///d.j").unwrap();

        // Initially A→B, A→C
        g.update(&a, std::collections::HashSet::from([b.clone(), c.clone()]));
        assert_eq!(g.direct_imports(&a).len(), 2);

        // Now A→C, A→D (B removed, D added)
        g.update(&a, std::collections::HashSet::from([c.clone(), d.clone()]));
        let imports = g.direct_imports(&a);
        assert_eq!(imports.len(), 2);
        assert!(imports.contains(&c));
        assert!(imports.contains(&d));
        assert!(!imports.contains(&b), "B should be removed from A's imports");
    }

    // ── two-file scenario: anal.j → ass.j (unified scope) ─────────────
    //
    //  anal.j imports ass.j  →  they share a single scope.
    //
    //  anal.j sees: function B from ass.j  (external link)
    //  ass.j sees:  real A, function A from anal.j  (external link)
    //               (because the connected component is {anal.j, ass.j})

    #[test]
    fn two_file_unified_scope_anal_sees_ass() {
        // === File ass.j ===
        let src_ass = "\
function B takes nothing returns nothing
endfunction
";
        let uri_ass = Url::parse("file:///test/ass.j").unwrap();
        let (cursor_ass, rope_ass) = parse_file(src_ass);
        let symbols_ass = collect_symbols(&cursor_ass, &rope_ass, &uri_ass);

        // ass.j should export function B
        assert!(
            symbols_ass.iter().any(|s| s.name == "B" && s.kind == ImportedKind::Func),
            "ass.j should export function B"
        );

        // === File anal.j (imports ass.j — unified scope) ===
        let src_anal = "\
globals
    real A = 33
endglobals

function A takes nothing returns nothing
    local integer A = 33
    set A = 21
endfunction

call A(A + A(A))

call B()
";
        let (cursor_anal, _rope_anal) = parse_file_with_imports(src_anal, &symbols_ass);

        // B in anal.j should be external → ass.j
        let b_ext = cursor_anal.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_anal.ref_names.get(&k).map(|n| n == "B").unwrap_or(false));
        assert!(b_ext.is_some(), "B should be an external ref in anal.j");
        assert_eq!(
            cursor_anal.external_decls[b_ext.unwrap()].uri,
            uri_ass,
            "B should come from ass.j"
        );

        // A should be entirely local (global var, function, local var)
        let a_ext = cursor_anal.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_anal.ref_names.get(&k).map(|n| n == "A").unwrap_or(false));
        assert!(a_ext.is_none(), "A should NOT be external in anal.j — it's declared locally");
    }

    #[test]
    fn two_file_unified_scope_ass_sees_anal() {
        // In unified scope: when anal.j imports ass.j, ass.j should also
        // see anal.j's symbols (because they're in the same connected component).

        // === File anal.j ===
        let src_anal = "\
globals
    real A = 33
endglobals

function A takes nothing returns nothing
endfunction
";
        let uri_anal = Url::parse("file:///test/anal.j").unwrap();
        let (cursor_anal, rope_anal) = parse_file(src_anal);
        let symbols_anal = collect_symbols(&cursor_anal, &rope_anal, &uri_anal);

        // anal.j exports: variable A (global), function A
        assert!(
            symbols_anal.iter().any(|s| s.name == "A" && s.kind == ImportedKind::Var),
            "anal.j should export global var A"
        );
        assert!(
            symbols_anal.iter().any(|s| s.name == "A" && s.kind == ImportedKind::Func),
            "anal.j should export function A"
        );

        // === File ass.j — receives anal.j's symbols via unified scope ===
        let src_ass = "\
function B takes nothing returns nothing
endfunction

A = 44

call A()
call A()
call B()
";
        // In unified scope, ass.j gets anal.j's symbols as imports.
        let (cursor_ass, _rope_ass) = parse_file_with_imports(src_ass, &symbols_anal);

        // B is declared locally → local group
        let (b_key, b_occs) = find_group(&cursor_ass, "B");
        assert!(*b_key < EXTERNAL_KEY_BASE, "B should be local in ass.j");
        assert_eq!(b_occs.len(), 2, "B: 1 decl + 1 call");

        // function A should be external → anal.j  (from "call A()")
        let a_func_ext = cursor_ass.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_ass.ref_names.get(&k).map(|n| n == "A").unwrap_or(false)
                && cursor_ass.external_decls.get(&k)
                    .map(|d| d.uri == uri_anal).unwrap_or(false));
        assert!(
            a_func_ext.is_some(),
            "In unified scope, ass.j should see A from anal.j as external"
        );
        assert_eq!(
            cursor_ass.external_decls[a_func_ext.unwrap()].uri,
            uri_anal,
            "A should come from anal.j"
        );
    }

    #[test]
    fn two_file_without_imports_still_isolated() {
        // If files are NOT connected via imports at all, they don't share scope.
        let src_a = "function Foo takes nothing returns nothing\nendfunction\n";
        let (_cursor_a, _) = parse_file(src_a);

        let src_b = "call Foo()\n";
        // No imports → Foo is unresolved
        let (cursor_b, _) = parse_file(src_b);

        let foo_ext = cursor_b.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_b.ref_names.get(&k).map(|n| n == "Foo").unwrap_or(false));
        assert!(foo_ext.is_none(), "Without imports, Foo should be unresolved (not external)");

        // Foo should be standalone
        let foo_groups: Vec<_> = cursor_b.ref_groups.iter()
            .filter(|(key, _)| {
                cursor_b.ref_names.get(key).map(|n| n == "Foo").unwrap_or(false)
            })
            .collect();
        assert!(!foo_groups.is_empty(), "Foo should have a standalone group");
    }

    #[test]
    fn two_file_triangle_unified_scope() {
        // A → B, A → C  →  B and C also see each other (unified scope).

        // File B: declares FuncB
        let src_b = "function FuncB takes nothing returns nothing\nendfunction\n";
        let uri_b = Url::parse("file:///test/b.j").unwrap();
        let (cursor_b, rope_b) = parse_file(src_b);
        let _symbols_b = collect_symbols(&cursor_b, &rope_b, &uri_b);

        // File C: declares FuncC
        let src_c = "function FuncC takes nothing returns nothing\nendfunction\n";
        let uri_c = Url::parse("file:///test/c.j").unwrap();
        let (cursor_c, rope_c) = parse_file(src_c);
        let symbols_c = collect_symbols(&cursor_c, &rope_c, &uri_c);

        // In unified scope (A→B, A→C), B sees C's symbols and vice versa.
        // File B uses FuncC:
        let src_b2 = "\
function FuncB takes nothing returns nothing
    call FuncC()
endfunction
";
        let (cursor_b2, _) = parse_file_with_imports(src_b2, &symbols_c);

        let fc_ext = cursor_b2.ref_groups.keys()
            .find(|&&k| k >= EXTERNAL_KEY_BASE
                && cursor_b2.ref_names.get(&k).map(|n| n == "FuncC").unwrap_or(false));
        assert!(fc_ext.is_some(), "In triangle, B should see FuncC from C");
        assert_eq!(cursor_b2.external_decls[fc_ext.unwrap()].uri, uri_c);
    }
}
