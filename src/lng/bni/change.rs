use crate::lng::bni::parse::parse;
use crate::lng::bni::uri_map::PARSER_MAP;
use crate::lsp::text_document::TextDocumentContentChangeEvent;
use crate::util::uri_map::{LINE_LIST_MAP, TREE_MAP};
use log::info;
use std::error::Error;
use tree_sitter::InputEdit;
use url::Url;

pub async fn change(
    uri: &Url,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    {
        let mut line_list_map = LINE_LIST_MAP.lock().await;
        let mut tree_map = TREE_MAP.lock().await;

        let line_list = line_list_map.get_mut(uri).ok_or("no line list")?;
        let tree_old = tree_map.get_mut(uri).ok_or("no tree")?;

        for change in &changes {
            let start = &change.range.start;
            let end = &change.range.end;

            let start_byte = line_list
                .position_to_offset(start)
                .ok_or("no start position")?;

            let old_end_byte = line_list.position_to_offset(end).ok_or("no end position")?;

            line_list.apply_change(start, end, &change.text);

            let new_end_byte = start_byte + change.text.len();

            tree_old.edit(&InputEdit {
                start_byte,
                old_end_byte,
                new_end_byte,
                start_position: start.into(),
                old_end_position: end.into(),
                new_end_position: line_list.point_from_offset(new_end_byte),
            });
        }

        let tree_new = {
            let mut parser_map = PARSER_MAP.lock().await;
            let parser = parser_map.get_mut(uri).ok_or("no parser")?;
            parser
                .parse(line_list.to_text(), Some(tree_old))
                .ok_or("parse failed")?
        };

        tree_map.insert(uri.clone(), tree_new);
        info!("Updated tree for {}", uri);
    }
    parse(uri).await
}
