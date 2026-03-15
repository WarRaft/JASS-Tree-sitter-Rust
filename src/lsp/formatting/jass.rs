//! JASS formatter — produces `TextEdit`s that normalise whitespace.
//!
//! The formatter has two phases:
//!
//! 1. **Indentation** — walks the CST and assigns an indentation depth to
//!    every line, emitting edits for leading whitespace that differs.
//!
//! 2. **Inline spacing** — walks the CST and normalises whitespace between
//!    tokens on the same line: spaces around operators, after commas,
//!    around `=`, no extra space inside `()` / `[]`, single space between
//!    keywords and their operands, etc.

use crate::lng::jass::kind::Kind;
use crate::lng::jass::uri_map::TREE_MAP;
use crate::lsp::formatting::lsp::{FormattingOptions, TextEdit};
use crate::util::roper::uri_map::ROPE_MAP;
use lapce_xi_rope::Rope;
use url::Url;

/// Compute formatting edits for a JASS file.
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

    // ── Phase 1: Compute desired indent level per line ──────────────────
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

    // ── Phase 3: Leading / trailing whitespace edits ────────────────────
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
                range: crate::lsp::range::Range {
                    start: crate::lsp::position::Position { line: last_line, character: last_line_len },
                    end: crate::lsp::position::Position { line: last_line, character: last_line_len },
                },
                new_text: "\n".to_string(),
            });
        }
    }

    // ── Phase 4: Inline whitespace normalisation ────────────────────────
    collect_inline_edits(root, &lines, &mut edits);

    edits
}

/// Walk the CST recursively and compute the indent level for each line.
///
/// JASS indentation rules:
/// - `function` … `endfunction` → body at +1
/// - `globals` … `endglobals` → body at +1
/// - `if`/`elseif`/`else` … `endif` → body at +1
/// - `loop` … `endloop` → body at +1
fn compute_indent_levels(node: tree_sitter::Node, levels: &mut [i32]) {
    let kind = Kind::try_from(node.kind_id());

    match kind {
        Ok(Kind::FunctionStatement) => {
            // The `function` keyword line stays at parent indent.
            // Children between `function` line and `endfunction` get +1.
            mark_block_indent(node, levels, Kind::Function, Kind::Endfunction);
        }
        Ok(Kind::GlobalsBlock) => {
            mark_block_indent(node, levels, Kind::Globals, Kind::Endglobals);
        }
        Ok(Kind::IfStatement) => {
            mark_if_indent(node, levels);
        }
        Ok(Kind::LoopStatement) => {
            mark_block_indent(node, levels, Kind::Loop, Kind::Endloop);
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

/// For blocks delimited by an opener and closer keyword on their own lines,
/// indent all lines between the opener and closer by +1.
fn mark_block_indent(
    node: tree_sitter::Node,
    levels: &mut [i32],
    _opener_kind: Kind,
    _closer_kind: Kind,
) {
    let start_line = node.start_position().row;
    let end_line = node.end_position().row;

    // Everything between the first and last line gets +1
    if end_line > start_line {
        for line in (start_line + 1)..end_line {
            if line < levels.len() {
                levels[line] += 1;
            }
        }
    }
}

/// Handle JASS `if` statements which have `elseif`, `else` sub-blocks.
///
/// Structure:
/// ```text
/// if <cond> then         ← parent indent
///     <body>             ← +1
/// elseif <cond> then     ← parent indent
///     <body>             ← +1
/// else                   ← parent indent
///     <body>             ← +1
/// endif                  ← parent indent
/// ```
fn mark_if_indent(node: tree_sitter::Node, levels: &mut [i32]) {
    let mut boundary_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();

    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i as u32) {
            let ck = Kind::try_from(child.kind_id());
            match ck {
                Ok(Kind::Elseif) | Ok(Kind::Else) | Ok(Kind::Endif) => {
                    boundary_lines.insert(child.start_position().row);
                }
                _ => {}
            }
        }
    }

    let start_line = node.start_position().row;
    let end_line = node.end_position().row;

    if end_line > start_line {
        for line in (start_line + 1)..=end_line {
            if line < levels.len() {
                if !boundary_lines.contains(&line) {
                    levels[line] += 1;
                }
            }
        }
    }
}

// ── Phase 4 helpers: inline whitespace normalisation ────────────────────────

/// Is this `Kind` a JASS keyword that expects surrounding spaces?
fn is_keyword(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Loop
            | Kind::Endloop
            | Kind::Constant
            | Kind::Array
            | Kind::Not
            | Kind::Return
            | Kind::Exitwhen
            | Kind::Local
            | Kind::Set
            | Kind::Call
            | Kind::If
            | Kind::Then
            | Kind::Elseif
            | Kind::Else
            | Kind::Endif
            | Kind::Native
            | Kind::Takes
            | Kind::Nothing
            | Kind::Returns
            | Kind::Type
            | Kind::Extends
            | Kind::Function
            | Kind::Endfunction
            | Kind::Globals
            | Kind::Endglobals
            | Kind::And
            | Kind::Or
    )
}

/// Check if a `Result<Kind, _>` is a keyword.
fn is_keyword_opt(k: &Result<Kind, impl std::fmt::Debug>) -> bool {
    matches!(k, Ok(k) if is_keyword(*k))
}

/// Ensure the whitespace gap between two adjacent sibling nodes equals
/// `expected`.  Emits a `TextEdit` only when the actual gap differs and
/// consists entirely of whitespace (safety guard).
///
/// Both nodes must be on the same line — multi-line gaps are skipped.
fn ensure_gap(
    prev: tree_sitter::Node,
    next: tree_sitter::Node,
    lines: &[&str],
    expected: &str,
    edits: &mut Vec<TextEdit>,
) {
    let prev_end = prev.end_position();
    let next_start = next.start_position();

    // Only same-line gaps.
    if prev_end.row != next_start.row {
        return;
    }

    let line = prev_end.row;
    let start_col = prev_end.column;
    let end_col = next_start.column;

    if line >= lines.len() {
        return;
    }
    let line_str = lines[line];
    if start_col > line_str.len() || end_col > line_str.len() || start_col > end_col {
        return;
    }

    let actual = &line_str[start_col..end_col];

    if actual == expected {
        return;
    }

    // Safety: only fix if the gap is pure whitespace (or empty).
    if !actual.is_empty() && !actual.chars().all(|c| c == ' ' || c == '\t') {
        return;
    }

    edits.push(TextEdit {
        range: crate::lsp::range::Range {
            start: crate::lsp::position::Position {
                line,
                character: start_col,
            },
            end: crate::lsp::position::Position {
                line,
                character: end_col,
            },
        },
        new_text: expected.to_string(),
    });
}

/// Normalise spacing inside a binary or unary `expr` node.
///
/// Binary (3 children): `expr OP expr` — exactly one space around the operator.
/// Unary  (2 children): `OP expr` — `not` gets a space, `-`/`++`/`--` do not.
fn format_expr(node: tree_sitter::Node, lines: &[&str], edits: &mut Vec<TextEdit>) {
    let child_count = node.child_count();

    if child_count == 3 {
        // Binary: expr OP expr
        if let (Some(left), Some(op), Some(right)) =
            (node.child(0), node.child(1), node.child(2))
        {
            ensure_gap(left, op, lines, " ", edits);
            ensure_gap(op, right, lines, " ", edits);
        }
    } else if child_count == 2 {
        // Unary: OP expr
        if let (Some(op), Some(operand)) = (node.child(0), node.child(1)) {
            let ok = Kind::try_from(op.kind_id());
            match ok {
                // `not x` — space after keyword
                Ok(Kind::Not) => ensure_gap(op, operand, lines, " ", edits),
                // `-x`, `++x`, `--x` — no space
                Ok(Kind::Minus) | Ok(Kind::PlusPlus) | Ok(Kind::MinusMinus) => {
                    ensure_gap(op, operand, lines, "", edits);
                }
                _ => {}
            }
        }
    }
}

/// Walk the CST and emit inline-spacing `TextEdit`s.
///
/// For `Expr` nodes the dedicated [`format_expr`] handles operator spacing.
/// For everything else a set of generic rules covers commas, parentheses,
/// brackets, `=`, and keywords.
fn collect_inline_edits(
    node: tree_sitter::Node,
    lines: &[&str],
    edits: &mut Vec<TextEdit>,
) {
    let child_count = node.child_count();
    let kind = Kind::try_from(node.kind_id());

    // Never touch content inside string literals or comments.
    match kind {
        Ok(Kind::StringLiteral) | Ok(Kind::Comment) => return,
        _ => {}
    }

    if child_count >= 2 {
        if kind == Ok(Kind::Expr) {
            format_expr(node, lines, edits);
        } else {
            for i in 0..(child_count - 1) {
                let prev = match node.child(i as u32) {
                    Some(c) => c,
                    None => continue,
                };
                let next = match node.child((i + 1) as u32) {
                    Some(c) => c,
                    None => continue,
                };

                if prev.end_position().row != next.start_position().row {
                    continue;
                }

                let pk = Kind::try_from(prev.kind_id());
                let nk = Kind::try_from(next.kind_id());

                // Determine expected gap between these two siblings.
                let expected = if nk == Ok(Kind::Comma) {
                    // No space before `,`
                    Some("")
                } else if pk == Ok(Kind::Comma) {
                    // Space after `,`
                    Some(" ")
                } else if pk == Ok(Kind::LeftParen) {
                    // No space after `(`
                    Some("")
                } else if nk == Ok(Kind::RightParen) {
                    // No space before `)`
                    Some("")
                } else if nk == Ok(Kind::LeftParen) {
                    // No space before `(` (function call)
                    Some("")
                } else if pk == Ok(Kind::LeftBracket) {
                    // No space after `[`
                    Some("")
                } else if nk == Ok(Kind::RightBracket) {
                    // No space before `]`
                    Some("")
                } else if nk == Ok(Kind::LeftBracket) {
                    // No space before `[` (array index)
                    Some("")
                } else if nk == Ok(Kind::Equal) || pk == Ok(Kind::Equal) {
                    // Space around `=`
                    Some(" ")
                } else if is_keyword_opt(&pk) || is_keyword_opt(&nk) {
                    // Space around keywords
                    Some(" ")
                } else if pk == Ok(Kind::Id) && nk == Ok(Kind::Id) {
                    // Space between adjacent identifiers (type name, etc.)
                    Some(" ")
                } else {
                    None
                };

                if let Some(exp) = expected {
                    ensure_gap(prev, next, lines, exp, edits);
                }
            }
        }
    }

    // Recurse into children.
    for i in 0..child_count {
        if let Some(child) = node.child(i as u32) {
            collect_inline_edits(child, lines, edits);
        }
    }
}

/// Helper to get the Rope text content as a String.
#[allow(dead_code)]
fn rope_to_string(rope: &Rope) -> String {
    rope.slice_to_cow(0..rope.len()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply all formatting edits (indentation + inline) and return the result.
    fn format_jass(input: &str, tab_size: u32) -> String {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_jass::language().into())
            .unwrap();
        let tree = parser.parse(input, None).unwrap();
        let root = tree.root_node();

        let lines: Vec<&str> = input.split('\n').collect();
        let line_count = lines.len();

        // ── Phase 1: indent levels ──
        let mut indent_levels = vec![0i32; line_count];
        compute_indent_levels(root, &mut indent_levels);
        for lvl in indent_levels.iter_mut() {
            if *lvl < 0 {
                *lvl = 0;
            }
        }

        let tab_str = " ".repeat(tab_size as usize);

        // ── Phase 3: leading-whitespace edits ──
        let mut edits: Vec<TextEdit> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let desired_indent = tab_str.repeat(indent_levels[i] as usize);
            let cur_ws_len = line.len() - line.trim_start().len();
            let cur_ws = &line[..cur_ws_len];
            if cur_ws != desired_indent && cur_ws.chars().all(|c| c == ' ' || c == '\t') {
                edits.push(TextEdit::leading_ws(i, cur_ws_len, &desired_indent));
            }
        }

        // ── Phase 4: inline edits ──
        collect_inline_edits(root, &lines, &mut edits);

        // Apply edits in reverse order (bottom-right to top-left) so earlier
        // offsets stay valid while we mutate the string.
        edits.sort_by(|a, b| {
            b.range
                .start
                .line
                .cmp(&a.range.start.line)
                .then(b.range.start.character.cmp(&a.range.start.character))
        });

        let mut result = input.to_string();
        let result_lines: Vec<usize> = result.split('\n').scan(0usize, |off, l| {
            let start = *off;
            *off += l.len() + 1;
            Some(start)
        }).collect();

        for edit in &edits {
            let start = result_lines[edit.range.start.line] + edit.range.start.character;
            let end = result_lines[edit.range.end.line] + edit.range.end.character;
            result.replace_range(start..end, &edit.new_text);
        }

        result
    }

    // ── Indentation tests ───────────────────────────────────────────────

    #[test]
    fn test_function_indent() {
        let input = "function A takes nothing returns nothing\n\
                      local integer x = 0\n\
                      set x = 1\n\
                      endfunction";
        let expected = "function A takes nothing returns nothing\n\
                        \x20\x20\x20\x20local integer x = 0\n\
                        \x20\x20\x20\x20set x = 1\n\
                        endfunction";
        assert_eq!(format_jass(input, 4), expected);
    }

    #[test]
    fn test_globals_indent() {
        let input = "globals\n\
                      integer a = 0\n\
                      endglobals";
        let expected = "globals\n\
                        \x20\x20\x20\x20integer a = 0\n\
                        endglobals";
        assert_eq!(format_jass(input, 4), expected);
    }

    #[test]
    fn test_if_indent() {
        let input = "function A takes nothing returns nothing\n\
                      if true then\n\
                      set x = 1\n\
                      elseif false then\n\
                      set x = 2\n\
                      else\n\
                      set x = 3\n\
                      endif\n\
                      endfunction";
        let expected = "function A takes nothing returns nothing\n\
                        \x20\x20\x20\x20if true then\n\
                        \x20\x20\x20\x20\x20\x20\x20\x20set x = 1\n\
                        \x20\x20\x20\x20elseif false then\n\
                        \x20\x20\x20\x20\x20\x20\x20\x20set x = 2\n\
                        \x20\x20\x20\x20else\n\
                        \x20\x20\x20\x20\x20\x20\x20\x20set x = 3\n\
                        \x20\x20\x20\x20endif\n\
                        endfunction";
        assert_eq!(format_jass(input, 4), expected);
    }

    #[test]
    fn test_loop_indent() {
        let input = "function A takes nothing returns nothing\n\
                      loop\n\
                      exitwhen true\n\
                      endloop\n\
                      endfunction";
        let expected = "function A takes nothing returns nothing\n\
                        \x20\x20\x20\x20loop\n\
                        \x20\x20\x20\x20\x20\x20\x20\x20exitwhen true\n\
                        \x20\x20\x20\x20endloop\n\
                        endfunction";
        assert_eq!(format_jass(input, 4), expected);
    }

    #[test]
    fn test_comment_preserved_on_line() {
        let input = "function A takes nothing returns nothing\n\
                      // this is a comment\n\
                      set x = 1\n\
                      endfunction";
        let expected = "function A takes nothing returns nothing\n\
                        \x20\x20\x20\x20// this is a comment\n\
                        \x20\x20\x20\x20set x = 1\n\
                        endfunction";
        assert_eq!(format_jass(input, 4), expected);
    }

    // ── Inline formatting tests ─────────────────────────────────────────

    #[test]
    fn test_binary_expr_spacing() {
        let input = "function A takes nothing returns nothing\n\
                      set x = 1+2*3\n\
                      endfunction";
        let result = format_jass(input, 4);
        assert!(
            result.contains("set x = 1 + 2 * 3"),
            "Binary operators should have spaces: {result}"
        );
    }

    #[test]
    fn test_function_args_comma_spacing() {
        let input = "function A takes nothing returns nothing\n\
                      call Foo(x,y,z)\n\
                      endfunction";
        let result = format_jass(input, 4);
        assert!(
            result.contains("call Foo(x, y, z)"),
            "Commas in args should have space after: {result}"
        );
    }

    #[test]
    fn test_function_args_extra_space() {
        let input = "function A takes nothing returns nothing\n\
                      call Foo(x , y)\n\
                      endfunction";
        let result = format_jass(input, 4);
        assert!(
            result.contains("call Foo(x, y)"),
            "No space before comma, space after: {result}"
        );
    }

    #[test]
    fn test_parameter_list_comma_spacing() {
        let input = "function A takes integer x,real y returns nothing\nendfunction";
        let result = format_jass(input, 4);
        assert!(
            result.contains("takes integer x, real y"),
            "Parameter commas need space after: {result}"
        );
    }

    #[test]
    fn test_set_assignment_spacing() {
        let input = "function A takes nothing returns nothing\n\
                      set x=1\n\
                      endfunction";
        let result = format_jass(input, 4);
        assert!(
            result.contains("set x = 1"),
            "Assignment should have spaces around =: {result}"
        );
    }

    #[test]
    fn test_local_assignment_spacing() {
        let input = "function A takes nothing returns nothing\n\
                      local integer z=x+y\n\
                      endfunction";
        let result = format_jass(input, 4);
        assert!(
            result.contains("local integer z = x + y"),
            "Local assignment and expr should have spaces: {result}"
        );
    }

    #[test]
    fn test_no_space_inside_parens() {
        let input = "function A takes nothing returns nothing\n\
                      call Foo( x )\n\
                      endfunction";
        let result = format_jass(input, 4);
        assert!(
            result.contains("call Foo(x)"),
            "No spaces inside parentheses: {result}"
        );
    }

    #[test]
    fn test_no_space_before_paren() {
        let input = "function A takes nothing returns nothing\n\
                      call Foo (x)\n\
                      endfunction";
        let result = format_jass(input, 4);
        assert!(
            result.contains("call Foo(x)"),
            "No space before opening paren in call: {result}"
        );
    }

    #[test]
    fn test_array_index_no_space() {
        let input = "function A takes nothing returns nothing\n\
                      set arr [i] = 1\n\
                      endfunction";
        let result = format_jass(input, 4);
        assert!(
            result.contains("set arr[i] = 1"),
            "No spaces around array brackets: {result}"
        );
    }

    #[test]
    fn test_keyword_spacing() {
        let input = "function  A  takes  nothing  returns  nothing\nendfunction";
        let result = format_jass(input, 4);
        assert_eq!(
            result,
            "function A takes nothing returns nothing\nendfunction",
            "Keywords should have single spaces"
        );
    }

    #[test]
    fn test_already_formatted_inline() {
        let input = "function A takes integer x, real y returns nothing\n\
                      \x20\x20\x20\x20call Foo(x, y)\n\
                      \x20\x20\x20\x20set x = 1 + 2\n\
                      endfunction";
        let result = format_jass(input, 4);
        assert_eq!(result, input, "Already-formatted file should be unchanged");
    }

    #[test]
    fn test_complex_expression() {
        let input = "function A takes nothing returns nothing\n\
                      return x+y*z\n\
                      endfunction";
        let result = format_jass(input, 4);
        assert!(
            result.contains("return x + y * z"),
            "Complex expression should have spaces: {result}"
        );
    }
}

