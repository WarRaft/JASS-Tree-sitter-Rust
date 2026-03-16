//! Unified tree-sitter parser and tree storage for all languages.
//!
//! Previously each language (JASS, AngelScript, BNI) maintained its own
//! `PARSER_MAP` and `TREE_MAP`.  Since all three have identical declarations
//! they are now consolidated here — a single `DashMap<Url, Parser>` and
//! `DashMap<Url, Tree>` shared by every language.

use dashmap::DashMap;
use once_cell::sync::Lazy;
use tree_sitter::{Parser, Tree};
use url::Url;

/// Per-URI tree-sitter `Parser` (reused across edits for the same file).
pub static PARSER_MAP: Lazy<DashMap<Url, Parser>> = Lazy::new(DashMap::new);

/// Per-URI last-good `Tree` produced by tree-sitter.
pub static TREE_MAP: Lazy<DashMap<Url, Tree>> = Lazy::new(DashMap::new);

