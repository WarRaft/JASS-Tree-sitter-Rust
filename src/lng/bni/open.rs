use crate::lng::bni::parse::parse;
use crate::lng::bni::uri_map::{PARSER_MAP, TREE_MAP};
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_map::LNG_MAP;
use lapce_xi_rope::Rope;
use std::error::Error;

use tree_sitter::Parser;
use url::Url;

pub async fn open(uri: &Url, text: impl AsRef<[u8]>) -> Result<(), Box<dyn Error + Send + Sync>> {
    {
        let text = std::str::from_utf8(text.as_ref())?;
        let rope = Rope::from(text);

        let mut rope_map = ROPE_MAP.write().await;
        rope_map.insert(uri.clone(), rope);

        LNG_MAP.write().await.insert(uri.clone(), "bni".to_string());

        let new_tree = {
            let mut parser_map = PARSER_MAP.write().await;
            let parser = parser_map.entry(uri.clone()).or_insert_with(|| {
                let mut parser = Parser::new();
                parser
                    .set_language(&tree_sitter_bni::LANGUAGE.into())
                    .expect("Failed to set language");
                parser
            });

            parser.parse(text, None).expect("Failed to parse BNI text")
        };

        TREE_MAP.write().await.insert(uri.clone(), new_tree);
    }
    parse(uri).await
}
