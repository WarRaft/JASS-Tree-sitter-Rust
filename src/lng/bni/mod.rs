use crate::lsp::semantic::TokenType;
use crate::util::uri_map::URI_MAP;
use tree_sitter::Parser;
use url::Url;

pub fn parse(uri: &Url, text: impl AsRef<[u8]>) {
    let language = tree_sitter_bni::LANGUAGE;
    let mut parser = Parser::new();

    parser
        .set_language(&language.into())
        .expect("Error loading Bni parser");

    let mut map = URI_MAP.lock().unwrap();
    let entry = map.entry(&uri);

    let semantic = entry.semantic.clear();
    let old_tree = entry.tree;

    let new_tree = parser.parse(&text, None).unwrap();
    let root = new_tree.root_node();

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
