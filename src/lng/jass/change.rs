use crate::lng::jass::parse::{parse, parse_from_disk};
use crate::lng::jass::uri_map::{PARSER_MAP, TREE_MAP};
use crate::lsp::position::Position;
use crate::lsp::text_document::TextDocumentContentChangeEvent;
use crate::util::file_store::{new_cancel_token, publish_diagnostics, publish_diagnostics_many, send_refresh_all};
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_lock::{uri_lock, uri_unlock};
use log::error;
use std::error::Error;
use tree_sitter::InputEdit;
use url::Url;

pub async fn change(
    uri: &Url,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Cancel any in-flight parse for this URI before acquiring the lock.
    // This ensures a fast-typing user doesn't pile up stale parse tasks.
    new_cancel_token(uri);

    uri_lock(uri).await;

    if let Err(e) = _apply_changes(uri, changes) {
        uri_unlock(uri);
        return Err(e);
    }

    // parse will call uri_unlock
    let cascade = parse(uri).await?;

    // Push diagnostics for the current file.
    publish_diagnostics(uri).await;

    // Cascade re-parse: connected peers whose scope changed.
    // One-level deep only — we discard the cascade list from each peer
    // to avoid infinite loops.
    let mut reparsed_peers = Vec::new();
    for peer_uri in &cascade {
        if ROPE_MAP.contains_key(peer_uri) && TREE_MAP.contains_key(peer_uri) {
            // Peer is open — lock + re-parse to pick up the new symbols.
            uri_lock(peer_uri).await;
            if let Err(e) = parse(peer_uri).await {
                error!("cascade re-parse {}: {}", peer_uri, e);
            } else {
                reparsed_peers.push(peer_uri.clone());
            }
        } else {
            // Peer is closed — parse from disk to update diagnostics.
            // (parse_from_disk handles its own locking.)
            if let Err(e) = parse_from_disk(peer_uri).await {
                error!("cascade re-parse closed {}: {}", peer_uri, e);
            } else {
                reparsed_peers.push(peer_uri.clone());
            }
        }
    }

    // Push diagnostics for all cascade-affected peers at once.
    publish_diagnostics_many(&reparsed_peers).await;

    // Ask VS Code to re-request semantic tokens / diagnostics / inlay hints
    // for ALL open files.  Always sent — VS Code may have cancelled its own
    // pull-requests while the parse was running, leaving stale data.
    send_refresh_all().await;

    Ok(())
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
