use crate::lsp::text_document::TextDocumentContentChangeEvent;
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
    crate::util::change::apply_edits(uri, changes, true)
}

