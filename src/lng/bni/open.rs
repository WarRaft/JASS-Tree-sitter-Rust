use crate::lng::bni::parse::parse_and_notify;
use crate::lng::bni::uri_map::{PARSER_MAP, TREE_MAP};
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_map::LNG_URI_MAP;
use lapce_xi_rope::Rope;
use std::error::Error;
use tree_sitter::Parser;
use url::Url;

/// Synchronous initialisation: set up rope, tree-sitter parser and initial tree.
pub fn init(uri: &Url, text: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    let rope = Rope::from(text);
    ROPE_MAP.insert(uri.clone(), rope);
    LNG_URI_MAP.insert(uri.clone(), "bni".to_string());

    let mut parser = PARSER_MAP.entry(uri.clone()).or_insert_with(|| {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_bni::LANGUAGE.into())
            .expect("Failed to set language");
        parser
    });

    let new_tree = parser.parse(text, None).expect("Failed to parse BNI text");
    TREE_MAP.insert(uri.clone(), new_tree);
    Ok(())
}

/// Full open: init + parse.
#[allow(dead_code)]
pub async fn open(uri: &Url, text: impl AsRef<[u8]>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let text = std::str::from_utf8(text.as_ref())?;
    init(uri, text)?;
    parse_and_notify(uri).await
}
