use crate::lng::jass::parse::parse_and_notify;
use std::error::Error;
use std::time::Instant;
use url::Url;

/// Synchronous initialisation: set up rope, tree-sitter parser and initial tree.
///
/// **Must be called from the main message loop** (not from a spawned task) to
/// guarantee that edits for the same URI are applied in arrival order.
pub fn init(uri: &Url, text: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let started_at = Instant::now();
    crate::debug_log!("jass::open::init START uri={}, text_len={}", uri.path(), text.len());
    let result = crate::util::open::init(uri, text, "jass", tree_sitter_jass::language().into());
    crate::debug_log!(
        "jass::open::init END uri={}, result={}, elapsed_ms={}",
        uri.path(),
        if result.is_ok() { "OK" } else { "ERR" },
        started_at.elapsed().as_millis()
    );
    result
}

/// Full open: init + parse + cascade + diagnostics + refresh.
///
/// Used by the `Initialized` handler for re-scanning stale files.
/// For the normal `DidOpen` path prefer calling [`init`] inline followed
/// by spawning [`parse_and_notify`].
pub async fn open(uri: &Url, text: impl AsRef<[u8]>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let text = std::str::from_utf8(text.as_ref())?;
    init(uri, text)?;
    parse_and_notify(uri, None).await
}
