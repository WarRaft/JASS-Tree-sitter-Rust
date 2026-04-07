use crate::util::change::TextDocumentContentChangeEvent;
use std::error::Error;
use url::Url;

/// Synchronous: apply edits to the rope and do a full reparse.
pub fn apply_edits(
    uri: &Url,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // BNI doesn't cancel in-flight parses (no async parse tasks).
    crate::util::change::apply_edits(uri, changes, false)
}
