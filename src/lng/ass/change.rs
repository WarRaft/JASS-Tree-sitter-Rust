use crate::lsp::text_document::TextDocumentContentChangeEvent;
use std::error::Error;
use url::Url;

/// Synchronous: cancel any in-flight parse and apply edits to the rope,
/// then do a full reparse.
///
/// **Must be called from the main message loop** to preserve ordering.
pub fn apply_edits(
    uri: &Url,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    crate::util::change::apply_edits(uri, changes, true)
}

