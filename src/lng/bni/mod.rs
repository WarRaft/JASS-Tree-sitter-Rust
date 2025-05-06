use crate::lsp::semantic::TokenType;
use crate::lsp::text_document::TextDocumentContentChangeEvent;
use crate::util::uri_map::{UriMapEntry, URI_MAP};
use log::info;
use tree_sitter::{InputEdit, Parser};
use url::Url;

fn parse(entry: UriMapEntry) {
    let tree = match entry.tree {
        Some(ref t) => t,
        None => return,
    };

    let root = tree.root_node();
    let semantic = entry.semantic.clear();

    for i in 0..root.child_count() {
        let node = root.child(i).unwrap();
        let s = node.start_position();
        let e = node.end_position();

        if s.row != e.row {
            continue;
        }

        match node.kind() {
            "section" => {
                semantic.add(
                    s.row,
                    s.column,
                    e.column - s.column + 1,
                    TokenType::Keyword,
                    None,
                );
            }
            "item" => {
                semantic.add(
                    s.row,
                    s.column,
                    e.column - s.column + 1,
                    TokenType::String,
                    None,
                );
            }
            _ => {}
        }
    }
}

pub fn open(uri: &Url, text: impl AsRef<[u8]>) {
    let mut map = URI_MAP.lock().unwrap();
    let mut entry = map.entry(&uri);

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bni::LANGUAGE.into())
        .expect("Error loading Bni parser");

    let line_list = &mut entry.line_list;
    line_list.set_text(&text);

    entry.lng.replace("bni".to_string());
    entry.tree.replace(parser.parse(&text, None).unwrap());
    info!("open");

    parse(entry);
}

pub fn change(uri: &Url, changes: Vec<TextDocumentContentChangeEvent>) {
    let mut map = URI_MAP.lock().unwrap();
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
