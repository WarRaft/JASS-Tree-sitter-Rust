use crate::util::change::TextDocumentContentChangeEvent;
use std::error::Error;
use url::Url;

/// Synchronous: cancel any in-flight parse and apply edits to the rope,
/// then do a **full** tree-sitter reparse.
///
/// **Must be called from the main message loop** (not from a spawned task)
/// to guarantee that edits for the same URI are applied in arrival order.
pub fn apply_edits(
    uri: &Url,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    crate::util::change::apply_edits(uri, changes, true)
}
