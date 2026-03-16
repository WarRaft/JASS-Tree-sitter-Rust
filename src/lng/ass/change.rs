use crate::lng::ass::uri_map::{PARSER_MAP, TREE_MAP};
use crate::lsp::text_document::TextDocumentContentChangeEvent;
use crate::util::file_store::new_cancel_token;
use crate::util::roper::uri_map::ROPE_MAP;
use std::error::Error;
use url::Url;

/// Synchronous: cancel any in-flight parse and apply incremental edits.
///
/// **Must be called from the main message loop** to preserve ordering.
///
/// Uses full reparse (not incremental) to avoid tree-sitter reusing stale
/// subtrees across statement boundaries after edits.
pub fn apply_edits(
    uri: &Url,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    new_cancel_token(uri);
    _apply_changes(uri, changes)
}

fn _apply_changes(
    uri: &Url,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut rope_entry = ROPE_MAP.get_mut(uri).ok_or("no rope")?;
    let rope = rope_entry.value_mut();

    let mut parser_entry = PARSER_MAP.get_mut(uri).ok_or("no parser")?;
    let parser = parser_entry.value_mut();

    for change in &changes {
        let start = &change.range.start;
        let end = &change.range.end;
        let new_text = &change.text;

        let start_byte = start.to_byte_offset(rope).ok_or("no start byte")?;
        let old_end_byte = end.to_byte_offset(rope).ok_or("no end byte")?;
        rope.edit(start_byte..old_end_byte, new_text);
    }

    let text = rope.to_string();
    let new_tree = parser.parse(&text, None).ok_or("parse failed")?;

    drop(rope_entry);
    drop(parser_entry);

    TREE_MAP.insert(uri.clone(), new_tree);

    Ok(())
}

