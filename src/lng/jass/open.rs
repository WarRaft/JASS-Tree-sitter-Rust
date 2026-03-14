use crate::lng::jass::parse::{parse, parse_from_disk};
use crate::lng::jass::uri_map::{PARSER_MAP, TREE_MAP};
use crate::util::file_store::{publish_diagnostics, publish_diagnostics_many, send_refresh_all};
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_map::LNG_URI_MAP;
use lapce_xi_rope::Rope;
use log::error;
use std::error::Error;
use tree_sitter::Parser;
use url::Url;
use crate::util::uri_lock::uri_lock;

pub async fn open(uri: &Url, text: impl AsRef<[u8]>) -> Result<(), Box<dyn Error + Send + Sync>> {
    {
        uri_lock(uri).await;

        let text = std::str::from_utf8(text.as_ref())?;
        let rope = Rope::from(text);

        ROPE_MAP.insert(uri.clone(), rope);
        LNG_URI_MAP.insert(uri.clone(), "jass".to_string());

        let mut parser = PARSER_MAP.entry(uri.clone()).or_insert_with(|| {
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_jass::language().into())
                .expect("Failed to set language");
            parser
        });

        let new_tree = parser.parse(text, None).expect("Failed to parse JASS text");
        TREE_MAP.insert(uri.clone(), new_tree);
    }

    let cascade = parse(uri).await?;

    // Push diagnostics for the current file.
    publish_diagnostics(uri).await;

    // Cascade re-parse: connected peers whose scope changed.
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
            if let Err(e) = parse_from_disk(peer_uri).await {
                error!("cascade re-parse closed {}: {}", peer_uri, e);
            } else {
                reparsed_peers.push(peer_uri.clone());
            }
        }
    }

    // Push diagnostics for all cascade-affected peers at once.
    publish_diagnostics_many(&reparsed_peers).await;

    // Ask VS Code to re-request data for all open files.
    send_refresh_all().await;

    Ok(())
}
