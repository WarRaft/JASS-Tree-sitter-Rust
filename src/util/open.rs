//! Shared `init` / `open` logic for all tree-sitter–based languages.
//!
//! Each language's `open.rs` becomes a thin adapter that passes its
//! `tree_sitter::Language` and language-ID string to the generic helpers
//! defined here.

use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::tree_map::{PARSER_MAP, TREE_MAP};
use crate::util::uri_map::LNG_URI_MAP;
use lapce_xi_rope::Rope;
use std::error::Error;
use tree_sitter::Parser;
use url::Url;

/// Synchronous initialisation: set up rope, tree-sitter parser and initial tree.
///
/// **Must be called from the main message loop** (not from a spawned task) to
/// guarantee that edits for the same URI are applied in arrival order.
///
/// # Arguments
///
/// * `uri` — document URI.
/// * `text` — full document text.
/// * `language_id` — LSP language identifier (e.g. `"jass"`, `"angelscript"`, `"bni"`).
/// * `ts_language` — the `tree_sitter::Language` for this file type.
pub fn init(
    uri: &Url,
    text: &str,
    language_id: &str,
    ts_language: tree_sitter::Language,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let rope = Rope::from(text);
    ROPE_MAP.insert(uri.clone(), rope);
    LNG_URI_MAP.insert(uri.clone(), language_id.to_string());

    let mut parser = PARSER_MAP.entry(uri.clone()).or_insert_with(|| {
        let mut p = Parser::new();
        p.set_language(&ts_language)
            .expect("Failed to set tree-sitter language");
        p
    });

    let new_tree = parser
        .parse(text, None)
        .expect("Failed to parse document text");
    TREE_MAP.insert(uri.clone(), new_tree);
    Ok(())
}

