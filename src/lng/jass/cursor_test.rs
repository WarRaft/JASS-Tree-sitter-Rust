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
        let mut ast = build_ast(tree.root_node());
        rewrite_imports(&mut ast, src.as_bytes());
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
    ) -> (&'a u32, &'a Vec<crate::lsp::ref_map::RawOccurrence>) {
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
    ) -> Vec<(&'a u32, &'a Vec<crate::lsp::ref_map::RawOccurrence>)> {
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
            assert!(!local_lines.contains(&1), "local group should NOT contain line 1 (global)");
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
            kind: ImportedKind::Func, origin_decl_key: None, return_type: None, type_name: None,
        }];
        with_cursor_imported(src, &imported, |c| {
            let ext_count = c.ref_groups.keys().filter(|&&k| k >= EXTERNAL_KEY_BASE).count();
            assert_eq!(ext_count, 0, "local A should shadow the import");
            let (_, occs) = find_group(c, "A");
            assert_eq!(occs.len(), 2, "A: 1 decl + 1 call");
        });
    }

    // ── import: same-name func + var resolve to separate external groups ──

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

    // ── import: real ass.j scenario ──────────────────────────────────────

    #[test]
    fn link_import_real_ass_j_scenario() {
        // Exact content of ass.j (minus directives which become SetDir)
        let src = "\
//set ref-tip 1
//set type-tip 1

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
            assert_eq!(all.len(), 3, "F: 1 decl + 2 calls");
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

    // ─── TypeMap tests ──────────────────────────────────────────────────

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

    // ─── //set directive tests ──────────────────────────────────────────

    #[test]
    fn set_type_tip_recognized() {
        let src = "//set type-tip 1\nglobals\n    integer x = 5\nendglobals\n";
        with_cursor(src, |c| {
            assert_eq!(c.file_settings.get("type-tip").map(|v| v.as_str()), Some("1"));
        });
    }

    #[test]
    fn set_type_tip_off() {
        let src = "//set type-tip 0\nglobals\n    integer x = 5\nendglobals\n";
        with_cursor(src, |c| {
            assert_eq!(c.file_settings.get("type-tip").map(|v| v.as_str()), Some("0"));
        });
    }

    #[test]
    fn set_bool_invalid_value_warns() {
        let src = "//set ref-tip maybe\nglobals\n    integer x = 5\nendglobals\n";
        with_cursor(src, |c| {
            // The value is still stored (for forward-compat)
            assert_eq!(c.file_settings.get("ref-tip").map(|v| v.as_str()), Some("maybe"));
            // But a warning diagnostic should be emitted
            let has_warning = c.diagnostics.iter().any(|d| {
                d.message.contains("Invalid value") && d.message.contains("ref-tip")
            });
            assert!(has_warning, "should warn about invalid bool value, diagnostics: {:?}",
                c.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>());
        });
    }

    #[test]
    fn set_def_registry_has_type_tip() {
        let def = crate::lng::directive::find_set_def("type-tip");
        assert!(def.is_some(), "type-tip should be in SET_DEFS");
        let def = def.unwrap();
        assert_eq!(def.kind, crate::lng::directive::SetValueKind::Bool);
        assert_eq!(def.default, "0");
    }

    #[test]
    fn set_def_registry_has_all_known_keys() {
        for key in &["ref-tip", "type-tip", "build-jass", "build-as"] {
            assert!(
                crate::lng::directive::find_set_def(key).is_some(),
                "SET_DEFS should contain {:?}",
                key
            );
        }
    }

    #[test]
    fn set_validate_bool_accepts_0_and_1() {
        let def = crate::lng::directive::find_set_def("ref-tip").unwrap();
        assert!(crate::lng::directive::validate_set_value(def, "0").is_none());
        assert!(crate::lng::directive::validate_set_value(def, "1").is_none());
        assert!(crate::lng::directive::validate_set_value(def, "yes").is_some());
        assert!(crate::lng::directive::validate_set_value(def, "").is_some());
    }

    #[test]
    fn set_validate_path_accepts_anything() {
        let def = crate::lng::directive::find_set_def("build-jass").unwrap();
        assert!(crate::lng::directive::validate_set_value(def, "./output.j").is_none());
        assert!(crate::lng::directive::validate_set_value(def, "C:\\build\\out.j").is_none());
    }

    // ─── Expression-level type hint tests ──────────────────────────────

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

    // ======================================================================
    //  Cross-file variable linking (ass.j → anal.j)
    // ======================================================================
    //
    //  anal.j declares `real A = 33` + `function A`.
    //  ass.j uses `A = 44` (bare set) and `call A()`.
    //
    //  When parsing ass.j with `A` imported as both Var and Func,
    //  the bare-set `A` must resolve to the imported *variable* and
    //  `call A()` must resolve to the imported *function*.

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

    // ── VarStmt at top-level must export to file_symbols.globals ──────────
    //
    //  `real A = 33` at top level (VarStmt, NOT inside `globals` block)
    //  should appear in `file_symbols.globals` so the scope resolver
    //  exports it to importing files.

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

    // ======================================================================
    //  Unknown type — impossible type combinations
    // ======================================================================

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

    // ======================================================================
    //  Compile-time value display on declaration hints
    // ======================================================================

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

    // ─── Undeclared variables & unknown type propagation ────────────────

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

    // ─── Concrete type mismatch detection ─────────────────────────────────

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

    // ─── Forward reference resolution ───────────────────────────────────

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

    // ─── Handle leak detection tests ─────────────────────────────────────

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

    // ─── VarStmt scope tests ─────────────────────────────────────────

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

    // ─── Return diagnostic range test ────────────────────────────────

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

    // ─── //ignore directive tests ─────────────────────────────────────

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
}
