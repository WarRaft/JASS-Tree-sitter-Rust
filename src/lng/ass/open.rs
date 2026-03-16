use crate::lng::ass::parse::parse_and_notify;
use std::error::Error;
use url::Url;

/// Synchronous initialisation: set up rope, tree-sitter parser and initial tree.
pub fn init(uri: &Url, text: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    crate::util::open::init(uri, text, "angelscript", tree_sitter_as::language().into())
}

/// Full open: init + parse + diagnostics + refresh.
#[allow(dead_code)]
pub async fn open(uri: &Url, text: impl AsRef<[u8]>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let text = std::str::from_utf8(text.as_ref())?;
    init(uri, text)?;
    parse_and_notify(uri).await
}
