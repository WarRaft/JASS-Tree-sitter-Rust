use crate::lng::jass::parse::parse_and_notify;
use std::error::Error;
use url::Url;

/// Synchronous initialisation: set up rope, tree-sitter parser and initial tree.
///
/// **Must be called from the main message loop** (not from a spawned task) to
/// guarantee that edits for the same URI are applied in arrival order.
pub fn init(uri: &Url, text: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    crate::util::open::init(uri, text, "jass", tree_sitter_jass::language().into())
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
