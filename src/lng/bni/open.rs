use crate::lng::bni::parse::parse;
use crate::lng::bni::uri_map::PARSER_MAP;
use crate::util::line_list::LineList;
use crate::util::uri_map::{LINE_LIST_MAP, LNG_MAP, TREE_MAP};
use std::error::Error;
use tree_sitter::Parser;
use url::Url;

pub async fn open(uri: &Url, text: impl AsRef<[u8]>) -> Result<(), Box<dyn Error + Send + Sync>> {
    {
        let text = text.as_ref();

        let mut line_list_map = LINE_LIST_MAP.lock().await;
        let line_list = line_list_map
            .entry(uri.clone())
            .or_insert_with(LineList::new);

        line_list.set_text(std::str::from_utf8(text).expect("Invalid UTF-8"));

        LNG_MAP.lock().await.insert(uri.clone(), "bni".to_string());

        let text_str = std::str::from_utf8(text).expect("Invalid UTF-8");

        let new_tree = {
            let mut parser_map = PARSER_MAP.lock().await;
            let parser = parser_map.entry(uri.clone()).or_insert_with(|| {
                let mut parser = Parser::new();
                parser
                    .set_language(&tree_sitter_bni::LANGUAGE.into())
                    .expect("Failed to set language");
                parser
            });

            parser
                .parse(text_str, None)
                .expect("Failed to parse BNI text")
        };

        TREE_MAP.lock().await.insert(uri.clone(), new_tree);
    }
    parse(uri).await
}
