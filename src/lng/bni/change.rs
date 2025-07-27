use crate::lsp::text_document::TextDocumentContentChangeEvent;
use crate::util::uri_map::URI_MAP;
use tree_sitter::{InputEdit, Parser};
use url::Url;

pub async fn change(uri: &Url, changes: Vec<TextDocumentContentChangeEvent>) {
    let mut map = URI_MAP.lock().await;
    let entry = map.entry(uri);

    let line_list = entry.line_list;
    let tree = entry.tree.as_mut().unwrap();

    for change in changes {
        let range = &change.range;
        let new_text = &change.text;

        let start = &range.start;
        let end = &range.end;

        let start_byte = line_list.position_to_offset(start).unwrap();
        let old_end_byte = line_list.position_to_offset(end).unwrap();

        line_list.apply_change(start, end, new_text);

        let new_end_byte = start_byte + new_text.len();

        let edit = InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position: start.point(),
            old_end_position: end.point(),
            new_end_position: line_list.point_from_offset(new_end_byte),
        };
        tree.edit(&edit);
    }

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bni::LANGUAGE.into())
        .unwrap();
    let new_text = line_list.to_text();
    let new_tree = parser.parse(&new_text, None).unwrap();
    entry.tree.replace(new_tree);
    //parse(&uri);
}
