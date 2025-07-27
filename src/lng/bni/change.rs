use crate::lng::bni::parse::parse;
use crate::lsp::text_document::TextDocumentContentChangeEvent;
use crate::util::uri_map::{LINE_LIST_MAP, PARSER_MAP, TREE_MAP};
use tree_sitter::InputEdit;
use url::Url;

pub async fn change(uri: &Url, changes: Vec<TextDocumentContentChangeEvent>) {
    let (text, old_tree) = {
        let mut line_list_map = LINE_LIST_MAP.lock().await;
        let mut tree_map = TREE_MAP.lock().await;

        let line_list = line_list_map
            .get_mut(uri)
            .expect("LineList must exist for this URI");

        let tree = tree_map
            .get_mut(uri)
            .expect("Tree must exist for this URI")
            .as_mut()
            .expect("Tree is not initialized");

        for change in &changes {
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

        (line_list.to_text(), tree.clone())
    };

    let new_tree = {
        let mut parser_map = PARSER_MAP.lock().await;
        let parser = parser_map
            .get_mut(uri)
            .expect("Parser must exist for this URI");
        parser
            .parse(&text, Some(&old_tree))
            .expect("Failed to parse edited text")
    };

    {
        let mut tree_map = TREE_MAP.lock().await;
        tree_map.insert(uri.clone(), Some(new_tree));
    }

    parse(uri).await;
}
