use crate::http::position::Position;
use crate::http::ref_map::build_ref_map;
use crate::lng::jass::ast::*;
use crate::lng::jass::cursor::{Cursor, ImportedSymbol};
use lapce_xi_rope::Rope;

pub(super) fn with_cursor(src: &str, f: impl FnOnce(&Cursor)) {
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

pub(super) fn with_cursor_imported(
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

pub(super) fn ref_map_from(c: &Cursor, rope: &Rope) -> crate::http::ref_map::RefMap {
    build_ref_map(c.ref_groups.clone(), c.ref_names.clone(), c.external_decls.clone(), rope)
}

pub(super) fn find_group<'a>(
    cursor: &'a Cursor,
    name: &str,
) -> (&'a u32, &'a Vec<crate::http::ref_map::RawOccurrence>) {
    let mut found: Vec<_> = cursor
        .ref_groups
        .iter()
        .filter(|(k, _)| cursor.ref_names.get(k).map(|n| n == name).unwrap_or(false))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected exactly 1 group for {:?}, got {} (keys: {:?})",
        name,
        found.len(),
        cursor
            .ref_names
            .iter()
            .filter(|(_, v)| v.as_str() == name)
            .collect::<Vec<_>>()
    );
    found.pop().unwrap()
}

pub(super) fn find_groups<'a>(
    cursor: &'a Cursor,
    name: &str,
) -> Vec<(&'a u32, &'a Vec<crate::http::ref_map::RawOccurrence>)> {
    cursor
        .ref_groups
        .iter()
        .filter(|(k, _)| cursor.ref_names.get(k).map(|n| n == name).unwrap_or(false))
        .collect()
}

pub(super) fn assert_span_at(
    rm: &crate::http::ref_map::RefMap,
    rope: &Rope,
    line: usize,
    ch: usize,
    desc: &str,
) {
    let byte = Position { line, character: ch }
        .to_byte_offset(rope)
        .unwrap_or_else(|| panic!("{}: position ({},{}) has no byte offset", desc, line, ch));
    let key = rm.decl_key_at(byte);
    assert!(
        key.is_some(),
        "{}: no span at byte {} (L{}:{}). spans: {:?}",
        desc,
        byte,
        line,
        ch,
        rm.spans
            .iter()
            .map(|s| (s.start_byte, s.end_byte, s.decl_key))
            .collect::<Vec<_>>()
    );
}

