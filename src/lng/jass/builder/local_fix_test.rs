#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::http::diagnostic::{Diagnostic, DiagnosticCode};
    use crate::http::position::Position;
    use crate::http::range::Range;
    use crate::lng::jass::ast::{build_ast, rewrite_imports};

    fn build_index(src: &str) -> AstFixIndex {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .expect("Failed to set language");
        let tree = parser.parse(src, None).expect("Failed to parse");
        let mut ast = build_ast(tree.root_node());
        rewrite_imports(&mut ast, src.as_bytes());
        build_ast_fix_index(&ast, src)
    }

    #[test]
    fn apply_line_insert_and_replace() {
        let src = "function A takes nothing returns nothing\n    return x\nendfunction\n";
        let edits = vec![
            LineEdit {
                start_line: 0,
                end_line: 0,
                new_text: "globals\n    integer g\nendglobals\n\n".to_string(),
            },
            LineEdit {
                start_line: 2,
                end_line: 2,
                new_text: "    set x = null\n".to_string(),
            },
        ];

        let out = apply_line_edits(src, &edits);
        assert!(out.contains("globals\n    integer g\nendglobals\n\nfunction A"));
        assert!(out.contains("    set x = null\nendfunction"));
    }

    #[test]
    fn leak_insert_before_return() {
        let src = "function A takes nothing returns nothing\n    return\nendfunction\n";
        let index = build_index(src);

        let diag = Diagnostic {
            range: Range {
                start: Position {
                    line: 1,
                    character: 4,
                },
                end: Position {
                    line: 1,
                    character: 10,
                },
            },
            code: Some(DiagnosticCode::String("leak".into())),
            data: Some(serde_json::json!({
                "leak_var": "u",
                "leak_kind": "return",
                "leak_type": "unit",
                "func_name": "A"
            })),
            ..Diagnostic::default()
        };

        let edit = leak_text_edit(&diag, &index).expect("expected leak edit");
        assert_eq!(edit.start_line, 1);
        assert_eq!(edit.end_line, 1);
        assert_eq!(edit.new_text, "    set u = null\n");
    }

    #[test]
    fn returned_local_default_uses_local_temp() {
        let src = "function A takes nothing returns unit\n    return u\nendfunction\n";
        let index = build_index(src);

        let diag = Diagnostic {
            range: Range {
                start: Position { line: 1, character: 4 },
                end: Position { line: 1, character: 12 },
            },
            code: Some(DiagnosticCode::String("leak".into())),
            data: Some(serde_json::json!({
                "leak_var": "u",
                "leak_kind": "return",
                "leak_type": "unit",
                "func_name": "A",
                "returned_local": true
            })),
            ..Diagnostic::default()
        };

        let edits = returned_local_edits(&diag, &index, LeakFixMethod::LocalTemp);
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().any(|e| e.new_text == "    local unit A_u_ret\n"));
        assert!(edits.iter().any(|e| e.new_text.contains("set A_u_ret = u\n    set u = null\n    return A_u_ret\n")));
    }

    #[test]
    fn returned_local_nolocal_uses_global_temp() {
        let src = "function A takes nothing returns unit\n    return u\nendfunction\n";
        let index = build_index(src);

        let diag = Diagnostic {
            range: Range {
                start: Position { line: 1, character: 4 },
                end: Position { line: 1, character: 12 },
            },
            code: Some(DiagnosticCode::String("leak".into())),
            data: Some(serde_json::json!({
                "leak_var": "u",
                "leak_kind": "return",
                "leak_type": "unit",
                "func_name": "A",
                "returned_local": true
            })),
            ..Diagnostic::default()
        };

        let edits = returned_local_edits(&diag, &index, LeakFixMethod::GlobalTemp);
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().any(|e| e.new_text == "globals\n    unit A_u\nendglobals\n\n"));
        assert!(edits.iter().any(|e| e.new_text.contains("set A_u = u\n    set u = null\n    return A_u\n")));
    }
}
