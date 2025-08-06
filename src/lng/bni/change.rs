use crate::lng::bni::parse::parse;
use crate::lng::bni::uri_map::{PARSER_MAP, TREE_MAP};
use crate::lsp::position::Position;
use crate::lsp::text_document::TextDocumentContentChangeEvent;
use crate::util::roper::uri_map::ROPE_MAP;
use lapce_xi_rope::Rope;
use std::error::Error;
use tree_sitter::{InputEdit, Tree};
use url::Url;

pub async fn change(
    uri: &Url,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    {
        let mut parser_map = PARSER_MAP.write().await;
        let mut tree_map = TREE_MAP.write().await;
        let mut rope_map = ROPE_MAP.write().await;

        let rope: &mut Rope = rope_map.get_mut(uri).ok_or("no rope")?;
        let tree: &mut Tree = tree_map.get_mut(uri).ok_or("no tree")?;

        for change in &changes {
            let start = &change.range.start;
            let end = &change.range.end;
            let new_text = &change.text;

            let start_byte = start.to_byte_offset(rope).ok_or("no start byte")?;
            let old_end_byte = end.to_byte_offset(rope).ok_or("no end byte")?;

            // edit Rope
            rope.edit(start_byte..old_end_byte, new_text);

            let new_end_byte = start_byte + new_text.len();
            let new_end_point =
                Position::from_byte_offset(rope, new_end_byte).ok_or("no new end point")?;

            tree.edit(&InputEdit {
                start_byte,
                old_end_byte,
                new_end_byte,
                start_position: start.into(),
                old_end_position: end.into(),
                new_end_position: new_end_point.into(),
            });
        }

        let parser = parser_map.get_mut(uri).ok_or("no parser")?;
        let tree_old = tree_map.get(uri).ok_or("no tree after edit")?;

        // Note: rope.to_string() может быть неэффективным, но tree-sitter требует &str
        let text = rope.to_string();
        let tree_new = parser.parse(&text, Some(tree_old)).ok_or("parse failed")?;

        tree_map.insert(uri.clone(), tree_new);
    }

    parse(uri).await
}
