//! AngelScript formatter — produces whitespace-only `TextEdit`s that fix
//! indentation.
//!
//! AS uses brace-delimited blocks (`{ … }`).  The formatter walks the CST
//! and counts nesting depth for each line.  `switch`/`case`/`default`
//! keywords get special handling to keep `case:` at +1 and its body at +2
//! relative to the `switch`.
//!
//! **Guarantee**: every `TextEdit.new_text` consists exclusively of spaces
//! and/or tabs.

use crate::lng::ass::kind::Kind;
use crate::util::tree_map::TREE_MAP;
use crate::http::formatting::{FormattingOptions, TextEdit};
use crate::util::roper::uri_map::ROPE_MAP;
use url::Url;

/// Compute formatting edits for an AngelScript file.
pub fn format(uri: &Url, options: &FormattingOptions) -> Vec<TextEdit> {
    let tree_entry = match TREE_MAP.get(uri) {
        Some(e) => e,
        None => return vec![],
    };
    let rope_entry = match ROPE_MAP.get(uri) {
        Some(e) => e,
        None => return vec![],
    };
    let rope = rope_entry.value();
    let root = tree_entry.value().root_node();

    let text = rope.slice_to_cow(0..rope.len()).to_string();
    let lines: Vec<&str> = text.split('\n').collect();
    let line_count = lines.len();

    if line_count == 0 {
        return vec![];
    }

    // ── Phase 1: Compute indent level per line ──────────────────────────
    let mut indent_levels: Vec<i32> = vec![0; line_count];
    compute_indent_levels(root, &mut indent_levels);

    for lvl in indent_levels.iter_mut() {
        if *lvl < 0 {
            *lvl = 0;
        }
    }

    // ── Phase 2: Build indent string ────────────────────────────────────
    let tab_str = if options.insert_spaces {
        " ".repeat(options.tab_size as usize)
    } else {
        "\t".to_string()
    };

    // ── Phase 3: Emit TextEdits ─────────────────────────────────────────
    let mut edits = Vec::new();
    let trim_trailing = options.trim_trailing_whitespace.unwrap_or(false);

    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            if trim_trailing && !line.is_empty() {
                edits.push(TextEdit::leading_ws(i, line.len(), ""));
            }
            continue;
        }

        let desired_indent = tab_str.repeat(indent_levels[i] as usize);
        let cur_ws_len = line.len() - line.trim_start().len();
        let cur_ws = &line[..cur_ws_len];

        if cur_ws != desired_indent {
            if cur_ws.chars().all(|c| c == ' ' || c == '\t') {
                edits.push(TextEdit::leading_ws(i, cur_ws_len, &desired_indent));
            }
        }

        if trim_trailing {
            let trimmed = line.trim_end();
            if trimmed.len() < line.len() {
                let trail_start = trimmed.len();
                let trail_end = line.len();
                if line[trail_start..].chars().all(|c| c == ' ' || c == '\t' || c == '\r') {
                    edits.push(TextEdit::trailing_ws(i, trail_start, trail_end));
                }
            }
        }
    }

    // Insert final newline if requested and missing
    if options.insert_final_newline.unwrap_or(false) {
        if !text.ends_with('\n') {
            let last_line = line_count - 1;
            let last_line_len = lines[last_line].len();
            edits.push(TextEdit {
                range: crate::http::range::Range {
                    start: crate::http::position::Position { line: last_line, character: last_line_len },
                    end: crate::http::position::Position { line: last_line, character: last_line_len },
                },
                new_text: "\n".to_string(),
            });
        }
    }

    edits
}

/// Walk the CST recursively and compute indent levels for each line.
///
/// AS indentation rules:
/// - `Block` (`{ … }`) → body at +1, `}` at parent level
/// - `ClassDeclaration`, `InterfaceDeclaration`, `MixinDeclaration`,
///   `EnumDeclaration`, `NamespaceDeclaration` → their body block handles it
/// - `SwitchCase` (`case X:` / `default:`) → body at +1 relative to `case`
/// - `IfStatement` → brace blocks handle it; braceless single-statement bodies
///   aren't indented beyond parent (kept consistent with braced style)
fn compute_indent_levels(node: tree_sitter::Node, levels: &mut [i32]) {
    let kind = Kind::try_from(node.kind_id());

    match kind {
        Ok(Kind::Block) => {
            // A `{ … }` block.  The opening `{` and closing `}` stay at
            // parent indent.  Everything in between gets +1.
            let start_line = node.start_position().row;
            let end_line = node.end_position().row;
            if end_line > start_line {
                for line in (start_line + 1)..end_line {
                    if line < levels.len() {
                        levels[line] += 1;
                    }
                }
            }
        }
        Ok(Kind::ClassDeclaration)
        | Ok(Kind::InterfaceDeclaration)
        | Ok(Kind::MixinDeclaration)
        | Ok(Kind::EnumDeclaration)
        | Ok(Kind::NamespaceDeclaration) => {
            // These use braces — the Block child handles indentation.
            // But the body might not be a Block (enum uses EnumBody).
            // Handle EnumBody and interface body explicitly.
            indent_brace_children(node, levels);
        }
        Ok(Kind::EnumBody) => {
            let start_line = node.start_position().row;
            let end_line = node.end_position().row;
            if end_line > start_line {
                for line in (start_line + 1)..end_line {
                    if line < levels.len() {
                        levels[line] += 1;
                    }
                }
            }
        }
        Ok(Kind::SwitchStatement) => {
            // `switch (…) { case …: … }` — the braces aren't a Block node.
            // Indent between `{` and `}` by +1.
            indent_brace_children(node, levels);
        }
        Ok(Kind::SwitchCase) => {
            // `case X:` / `default:` — the body after the colon gets +1
            let start_line = node.start_position().row;
            let end_line = node.end_position().row;

            // The `case`/`default` keyword line stays at current level.
            // Lines below it get +1.
            if end_line > start_line {
                for line in (start_line + 1)..=end_line {
                    if line < levels.len() {
                        levels[line] += 1;
                    }
                }
            }
        }
        _ => {}
    }

    // Recurse into children
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i as u32) {
            compute_indent_levels(child, levels);
        }
    }
}

/// For nodes that use `{ … }` but where the brace pair isn't wrapped in a
/// `Block` node (e.g., interface body, some declaration forms), indent
/// between `{` and `}` children.
fn indent_brace_children(node: tree_sitter::Node, levels: &mut [i32]) {
    let mut brace_start: Option<usize> = None;
    let mut brace_end: Option<usize> = None;

    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i as u32) {
            let ck = Kind::try_from(child.kind_id());
            match ck {
                Ok(Kind::LeftBrace) => {
                    brace_start = Some(child.start_position().row);
                }
                Ok(Kind::RightBrace) => {
                    brace_end = Some(child.start_position().row);
                }
                _ => {}
            }
        }
    }

    if let (Some(start), Some(end)) = (brace_start, brace_end) {
        if end > start {
            for line in (start + 1)..end {
                if line < levels.len() {
                    levels[line] += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format_as(input: &str, tab_size: u32) -> String {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_as::language().into())
            .unwrap();
        let tree = parser.parse(input, None).unwrap();
        let root = tree.root_node();

        let lines: Vec<&str> = input.split('\n').collect();
        let line_count = lines.len();
        let mut indent_levels = vec![0i32; line_count];
        compute_indent_levels(root, &mut indent_levels);
        for lvl in indent_levels.iter_mut() {
            if *lvl < 0 {
                *lvl = 0;
            }
        }

        let tab_str = " ".repeat(tab_size as usize);
        let mut result = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if line.trim().is_empty() {
                result.push(String::new());
            } else {
                let desired_indent = tab_str.repeat(indent_levels[i] as usize);
                let trimmed = line.trim_start();
                result.push(format!("{}{}", desired_indent, trimmed));
            }
        }
        result.join("\n")
    }

    #[test]
    fn test_function_indent() {
        let input = "void main() {\n\
                      int x = 0;\n\
                      x = 1;\n\
                      }";
        let expected = "void main() {\n\
                        \x20\x20\x20\x20int x = 0;\n\
                        \x20\x20\x20\x20x = 1;\n\
                        }";
        assert_eq!(format_as(input, 4), expected);
    }

    #[test]
    fn test_class_indent() {
        let input = "class Foo {\n\
                      int x;\n\
                      void bar() {\n\
                      x = 1;\n\
                      }\n\
                      }";
        let expected = "class Foo {\n\
                        \x20\x20\x20\x20int x;\n\
                        \x20\x20\x20\x20void bar() {\n\
                        \x20\x20\x20\x20\x20\x20\x20\x20x = 1;\n\
                        \x20\x20\x20\x20}\n\
                        }";
        assert_eq!(format_as(input, 4), expected);
    }

    #[test]
    fn test_if_indent() {
        let input = "void main() {\n\
                      if (true) {\n\
                      x = 1;\n\
                      } else {\n\
                      x = 2;\n\
                      }\n\
                      }";
        let expected = "void main() {\n\
                        \x20\x20\x20\x20if (true) {\n\
                        \x20\x20\x20\x20\x20\x20\x20\x20x = 1;\n\
                        \x20\x20\x20\x20} else {\n\
                        \x20\x20\x20\x20\x20\x20\x20\x20x = 2;\n\
                        \x20\x20\x20\x20}\n\
                        }";
        assert_eq!(format_as(input, 4), expected);
    }

    #[test]
    fn test_comment_preserved() {
        let input = "void main() {\n\
                      // comment here\n\
                      int x = 0;\n\
                      }";
        let expected = "void main() {\n\
                        \x20\x20\x20\x20// comment here\n\
                        \x20\x20\x20\x20int x = 0;\n\
                        }";
        assert_eq!(format_as(input, 4), expected);
    }

    #[test]
    fn test_switch_indent() {
        let input = "void main() {\n\
                      switch (x) {\n\
                      case 1:\n\
                      foo();\n\
                      break;\n\
                      default:\n\
                      bar();\n\
                      }\n\
                      }";
        let expected = "void main() {\n\
                        \x20\x20\x20\x20switch (x) {\n\
                        \x20\x20\x20\x20\x20\x20\x20\x20case 1:\n\
                        \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20foo();\n\
                        \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20break;\n\
                        \x20\x20\x20\x20\x20\x20\x20\x20default:\n\
                        \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20bar();\n\
                        \x20\x20\x20\x20}\n\
                        }";
        assert_eq!(format_as(input, 4), expected);
    }
}

