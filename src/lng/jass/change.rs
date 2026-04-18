use crate::util::change::TextDocumentContentChangeEvent;
use std::error::Error;
use std::time::Instant;
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
    let started_at = Instant::now();
    let change_count = changes.len();
    crate::debug_log!(
        "jass::change::apply_edits START uri={}, change_count={}",
        uri.path(),
        change_count
    );
    let result = crate::util::change::apply_edits(uri, changes, true);
    crate::debug_log!(
        "jass::change::apply_edits END uri={}, change_count={}, result={}, elapsed_ms={}",
        uri.path(),
        change_count,
        if result.is_ok() { "OK" } else { "ERR" },
        started_at.elapsed().as_millis()
    );
    result
}
