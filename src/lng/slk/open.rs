use crate::lng::slk::parse::parse_and_notify;
use std::error::Error;
use url::Url;

/// Synchronous initialisation: set up rope, tree-sitter parser and initial tree.
pub fn init(uri: &Url, text: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    crate::util::open::init(uri, text, "slk", tree_sitter_slk::LANGUAGE.into())
}

/// Full open: init + parse.
#[allow(dead_code)]
pub async fn open(uri: &Url, text: impl AsRef<[u8]>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let text = std::str::from_utf8(text.as_ref())?;
    init(uri, text)?;
    parse_and_notify(uri).await
}

