use crate::lng::bni::parse::parse;
use crate::util::uri_map::URI_MAP;
use tree_sitter::Parser;
use url::Url;

pub async fn open(uri: &Url, text: impl AsRef<[u8]>) {
    let mut map = URI_MAP.lock().await;
    let mut entry = map.entry(&uri);

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bni::LANGUAGE.into())
        .expect("Error loading Bni parser");

    let line_list = &mut entry.line_list;
    line_list.set_text(&text);

    entry.lng.replace("bni".to_string());
    entry.tree.replace(parser.parse(&text, None).unwrap());

    parse(entry);
}
