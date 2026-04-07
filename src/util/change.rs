//! Shared `apply_edits` logic for all tree-sitter–based languages.
//!
//! Each language's `change.rs` becomes a one-liner that delegates here.
//!
//! We always do a **full** reparse (`parser.parse(text, None)`) rather than
//! incremental (`parser.parse(text, Some(old_tree))`).  Tree-sitter's
//! incremental parser reuses subtrees based purely on byte-range overlap —
//! it does **not** re-evaluate context.  For example, inserting `//` at the
//! start of a line won't cause tree-sitter to re-parse the tokens that were
//! previously on that line, leading to stale / incorrect AST nodes.
//! Full reparse is the only way to guarantee correctness.

use crate::lsp::text_document::TextDocumentContentChangeEvent;
use crate::util::file_store::new_cancel_token;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::tree_map::{PARSER_MAP, TREE_MAP};
use std::error::Error;
use url::Url;

/// Synchronous: optionally cancel any in-flight parse, apply incremental
/// edits to the rope, then do a **full** tree-sitter reparse.
///
/// **Must be called from the main message loop** to preserve edit ordering.
pub fn apply_edits(
    uri: &Url,
    changes: Vec<TextDocumentContentChangeEvent>,
    cancel: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if cancel {
        new_cancel_token(uri);
    }

    let mut rope_entry = ROPE_MAP.get_mut(uri).ok_or("no rope")?;
    let rope = rope_entry.value_mut();

    let mut parser_entry = PARSER_MAP.get_mut(uri).ok_or("no parser")?;
    let parser = parser_entry.value_mut();

    // ── Apply edits to the rope ─────────────────────────────────────────
    for change in &changes {
        let start = &change.range.start;
        let end = &change.range.end;
        let new_text = &change.text;

        let start_byte = start.to_byte_offset(rope).ok_or("no start byte")?;
        let old_end_byte = end.to_byte_offset(rope).ok_or("no end byte")?;

        rope.edit(start_byte..old_end_byte, new_text);
    }

    // ── Full reparse ────────────────────────────────────────────────────
    let text = rope.to_string();
    let new_tree = parser.parse(&text, None).ok_or("parse failed")?;

    // Drop DashMap guards before insert to avoid deadlock.
    drop(rope_entry);
    drop(parser_entry);

    TREE_MAP.insert(uri.clone(), new_tree);

    Ok(())
}



