//! Shared `apply_edits` logic for all tree-sitter–based languages.
//!
//! Each language's `change.rs` becomes a one-liner that delegates here.

use crate::lsp::inlay_hint::lsp::InlayHint;
use crate::lsp::position::Position;
use crate::lsp::text_document::TextDocumentContentChangeEvent;
use crate::util::file_store::new_cancel_token;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::tree_map::{PARSER_MAP, TREE_MAP};
use std::error::Error;
use url::Url;

/// A single byte-level edit delta.
struct EditDelta {
    start_byte: usize,
    old_end_byte: usize,
    new_end_byte: usize,
}

/// Synchronous: optionally cancel any in-flight parse, apply incremental
/// edits to the rope, then do a **full** tree-sitter reparse.
///
/// Before editing the rope, snapshots **all** current inlay hints with
/// byte-offset anchors.  After the edits and reparse, each hint is
/// **snapped to its AST node** in the new tree: the delta-adjusted byte
/// offset is used as a rough target, then `descendant_for_byte_range`
/// finds the actual tree-sitter node and provides its exact position.
///
/// Because the full set of surviving hints is pushed atomically, the
/// client cache is replaced in one shot — hints never blink.
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

    // ── 1. Snapshot ALL hints with byte offsets (BEFORE edits) ───────────
    let mut hints = snapshot_all_hints(uri, rope);

    // ── 2. Apply edits, collect deltas ──────────────────────────────────
    let mut deltas: Vec<EditDelta> = Vec::with_capacity(changes.len());

    for change in &changes {
        let start = &change.range.start;
        let end = &change.range.end;
        let new_text = &change.text;

        let start_byte = start.to_byte_offset(rope).ok_or("no start byte")?;
        let old_end_byte = end.to_byte_offset(rope).ok_or("no end byte")?;
        let new_end_byte = start_byte + new_text.len();

        rope.edit(start_byte..old_end_byte, new_text);

        deltas.push(EditDelta {
            start_byte,
            old_end_byte,
            new_end_byte,
        });
    }

    // ── 3. Full reparse ─────────────────────────────────────────────────
    let text = rope.to_string();
    let new_tree = parser.parse(&text, None).ok_or("parse failed")?;

    // ── 4. Snap hints to AST nodes in the new tree & push ───────────────
    if !hints.is_empty() {
        snap_to_tree_and_push(uri, &deltas, &mut hints, &new_tree);
    }

    // Drop DashMap guards before insert to avoid deadlock.
    drop(rope_entry);
    drop(parser_entry);

    TREE_MAP.insert(uri.clone(), new_tree);

    Ok(())
}

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Collect **all** inlay hints for `uri` (ujapi + type-tip + ref-tip) and
/// ensure every hint has a valid `byte_offset` derived from the current rope.
fn snapshot_all_hints(uri: &Url, rope: &lapce_xi_rope::Rope) -> Vec<InlayHint> {
    use crate::lsp::inlay_hint::send::compute_all;

    let mut hints = compute_all(uri);

    for hint in &mut hints {
        if let Some(bo) = hint.position.to_byte_offset(rope) {
            hint.byte_offset = bo;
        }
    }

    hints
}

/// Shift byte offsets by edit deltas, then look up each hint's AST node in
/// the **new** tree-sitter tree.  The tree provides the exact `end_position`
/// — no accumulation of arithmetic error.
///
/// Hints whose anchor lands inside a deleted region, or whose AST node no
/// longer exists at the adjusted position, are silently dropped.
fn snap_to_tree_and_push(
    uri: &Url,
    deltas: &[EditDelta],
    hints: &mut Vec<InlayHint>,
    tree: &tree_sitter::Tree,
) {
    use crate::util::file_store::LSP_WRITER;

    // ── approximate adjustment via deltas ────────────────────────────────
    for delta in deltas {
        let old_len = delta.old_end_byte - delta.start_byte;
        let new_len = delta.new_end_byte - delta.start_byte;

        hints.retain_mut(|hint| {
            if hint.byte_offset < delta.start_byte {
                true
            } else if hint.byte_offset < delta.old_end_byte {
                false // inside deleted/replaced region
            } else {
                hint.byte_offset = hint.byte_offset - old_len + new_len;
                true
            }
        });
    }

    // ── snap to AST nodes ───────────────────────────────────────────────
    let root = tree.root_node();

    hints.retain_mut(|hint| {
        // The hint sits at the END of a node.  Look up the node whose
        // range contains the byte just before the anchor.
        let target = hint.byte_offset.saturating_sub(1);
        if let Some(node) = root.descendant_for_byte_range(target, target) {
            let end = node.end_position();
            hint.position = Position {
                line: end.row,
                character: end.column,
            };
            hint.byte_offset = node.end_byte();
            true
        } else {
            false
        }
    });

    // ── push ────────────────────────────────────────────────────────────
    let writer = match LSP_WRITER.get() {
        Some(w) => w.clone(),
        None => return,
    };

    let uri_str = uri.to_string();
    let json_hints = serde_json::to_value(&*hints).unwrap_or_default();

    tokio::spawn(async move {
        crate::lsp::send::send(
            &writer,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "custom/publishInlayHints",
                "params": {
                    "uri": uri_str,
                    "hints": json_hints
                }
            }),
        )
        .await;
    });
}

