use crate::lsp::text_document::TextDocumentContentChangeEvent;
use std::error::Error;
use url::Url;

/// Synchronous: cancel any in-flight parse and apply incremental edits to
/// the rope, then do a **full** tree-sitter reparse.
///
/// **Must be called from the main message loop** (not from a spawned task)
/// to guarantee that edits for the same URI are applied in arrival order.
///
/// We intentionally do a **full** reparse (`parser.parse(text, None)`) rather
/// than an incremental one (`parser.parse(text, Some(old_tree))`).  Tree-sitter's
/// incremental parser can sometimes reuse stale subtrees across statement
/// boundaries — e.g. adding `boolean T3` right after `boolean T1 = false and
/// true or true` may cause `boolean` to be parsed as an identifier instead of
/// a type keyword.  Full reparsing is sub-millisecond for typical JASS files
/// and eliminates this class of bugs.
pub fn apply_edits(
    uri: &Url,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    crate::util::change::apply_edits(uri, changes, true)
}
