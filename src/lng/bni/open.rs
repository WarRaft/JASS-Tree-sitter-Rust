use crate::lng::bni::parse::parse;
use crate::util::line_list::LineList;
use crate::util::uri_map::{LINE_LIST_MAP, LNG_MAP, PARSER_MAP, TREE_MAP};
use tree_sitter::Parser;
use url::Url;

pub async fn open(uri: &Url, text: impl AsRef<[u8]>) {
    let text = text.as_ref();

    {
        let mut line_list_map = LINE_LIST_MAP.lock().await;
        let line_list = line_list_map
            .entry(uri.clone())
            .or_insert_with(LineList::new);
        line_list.set_text(text);
    }

    {
        let mut lng_map = LNG_MAP.lock().await;
        lng_map.insert(uri.clone(), Some("bni".to_string()));
    }

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

    {
        let mut tree_map = TREE_MAP.lock().await;
        tree_map.insert(uri.clone(), Some(new_tree));
    }

    parse(uri).await;
}
