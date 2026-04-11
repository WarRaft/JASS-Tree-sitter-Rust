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

// ─── Language detection ─────────────────────────────────────────────────────

/// Returns `true` if the URI has a `.as` extension (AngelScript).
pub fn is_as_uri(uri: &Url) -> bool {
    uri.path().ends_with(".as")
}

// ─── Universal open ─────────────────────────────────────────────────────────

/// Open and parse a file from disk content, dispatching to the correct
/// language based on the URI extension.
///
/// * `.as` → `lng::ass::open::open`
/// * everything else → `lng::jass::open::open`
///
/// Use this instead of hardcoding `lng::jass::open::open` in places that
/// handle arbitrary URIs (file watcher, rescan, stale re-parse, etc.).
pub async fn open_by_uri(
    uri: &Url,
    content: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if is_as_uri(uri) {
        crate::lng::ass::open::open(uri, content).await
    } else {
        crate::lng::jass::open::open(uri, content).await
    }
}

/// Synchronous init dispatched by URI extension.
///
/// Sets up rope, tree-sitter parser and initial tree — no diagnostics.
/// Used by the first pass of rescan.
pub fn init_by_uri(
    uri: &Url,
    content: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if is_as_uri(uri) {
        crate::lng::ass::open::init(uri, content)
    } else {
        crate::lng::jass::open::init(uri, content)
    }
}

/// Init + parse without cascade.
///
/// Used by rescan pass 2: all symbols are already in the scope resolver,
/// so we only need to produce diagnostics and PARSE_CACHE snapshots
/// without triggering cascade re-parses of dependent files.
pub async fn parse_only_by_uri(
    uri: &Url,
    content: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    init_by_uri(uri, content)?;
    crate::util::parse::parse_by_uri(uri).await?;
    Ok(())
}

// ─── Synchronous init ───────────────────────────────────────────────────────

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

    let mut parser_entry = PARSER_MAP.entry(uri.clone()).or_insert_with(Parser::new);
    parser_entry
        .set_language(&ts_language)
        .expect("Failed to set tree-sitter language");

    let new_tree = parser_entry
        .parse(text, None)
        .expect("Failed to parse document text");
    TREE_MAP.insert(uri.clone(), new_tree);
    Ok(())
}
