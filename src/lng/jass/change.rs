use crate::lng::jass::parse::parse;
use crate::lng::jass::uri_map::{PARSER_MAP, TREE_MAP};
use crate::lsp::position::Position;
use crate::lsp::text_document::TextDocumentContentChangeEvent;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_lock::{uri_lock, uri_unlock};
use std::error::Error;
use tree_sitter::InputEdit;
use url::Url;

pub async fn change(
    uri: &Url,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    uri_lock(uri).await;

    if let Err(e) = _apply_changes(uri, changes) {
        uri_unlock(uri);
        return Err(e);
    }

    // parse will call uri_unlock
    parse(uri).await
}

fn _apply_changes(
    uri: &Url,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut rope_entry = ROPE_MAP.get_mut(uri).ok_or("no rope")?;
    let rope = rope_entry.value_mut();

    let mut tree_entry = TREE_MAP.get_mut(uri).ok_or("no tree")?;
    let tree = tree_entry.value_mut();

    let mut parser_entry = PARSER_MAP.get_mut(uri).ok_or("no parser")?;
    let parser = parser_entry.value_mut();

    for change in &changes {
        let start = &change.range.start;
        let end = &change.range.end;
        let new_text = &change.text;

        let start_byte = start.to_byte_offset(rope).ok_or("no start byte")?;
        let old_end_byte = end.to_byte_offset(rope).ok_or("no end byte")?;
        rope.edit(start_byte..old_end_byte, new_text);

        let new_end_byte = start_byte + new_text.len();
        let new_end_point =
            Position::from_byte_offset(rope, new_end_byte).ok_or("no new end point")?;

        tree.edit(&InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position: start.into(),
            old_end_position: end.into(),
            new_end_position: new_end_point.into(),
        });
    }

    let text = rope.to_string();
    let new_tree = parser.parse(&text, Some(&*tree)).ok_or("parse failed")?;

    // Drop guards before insert to avoid DashMap deadlock
    drop(rope_entry);
    drop(tree_entry);
    drop(parser_entry);

    TREE_MAP.insert(uri.clone(), new_tree);

    Ok(())
}
