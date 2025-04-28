use crate::lsp::semantic::TokenType;
use crate::lsp::semantic_hub::SEMANTIC_STORE;
use log::info;
use tree_sitter::Parser;
use url::Url;

pub fn parse(uri: &Url, text: impl AsRef<[u8]>) {
    let language = tree_sitter_bni::LANGUAGE;
    let mut parser = Parser::new();

    parser
        .set_language(&language.into())
        .expect("Error loading Bni parser");

    let tree = parser.parse(&text, None).unwrap();

    let root = tree.root_node();

    let mut store = SEMANTIC_STORE.lock().unwrap();
    let hub = store.hub(&uri);
    hub.clear();

    for i in 0..root.child_count() {
        let node = root.child(i).unwrap();
        let s = node.start_position();
        let e = node.end_position();

        if s.row != e.row {
            continue;
        }

        info!("kind:>{:?}<|{:?}|{:?}|", node.kind(), s, e);
        match node.kind() {
            "section" => {
                hub.add(
                    s.row,
                    s.column,
                    e.column - s.column + 1,
                    TokenType::Keyword,
                    None,
                );
            }
            "item" => {
                hub.add(
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

    info!("hubs1:");
    for (uri, hub) in &store.hubs {
        info!("uri = {:?}, hub = {:?}", uri, hub);
    }

    //info!("{:?}", hub.lines);
}
