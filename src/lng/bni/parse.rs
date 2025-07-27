use crate::lng::bni::kind::Kind;
use crate::lsp::semantic::TokenType;
use crate::lsp::semantic_hub::SemanticTokenHub;
use crate::util::uri_map::{SEMANTIC_MAP, TREE_MAP};
use log::error;
use url::Url;

pub async fn parse(uri: &Url) {
    let tree = {
        let tree_map = TREE_MAP.lock().await;
        match tree_map.get(uri) {
            Some(Some(tree)) => tree.clone(),
            _ => return,
        }
    };

    let mut semantic_map = SEMANTIC_MAP.lock().await;
    let semantic = semantic_map
        .entry(uri.clone())
        .or_insert_with(SemanticTokenHub::new);
    semantic.clear();

    let root = tree.root_node();
    for i in 0..root.child_count() {
        if let Some(node) = root.child(i) {
            match node.kind().parse::<Kind>() {
                Ok(kind) => match kind {
                    Kind::Section => {
                        semantic.add_node(&node, TokenType::Keyword, None);
                    }
                    Kind::Item => {
                        semantic.add_node(&node, TokenType::String, None);
                    }
                    _ => {}
                },
                Err(e) => error!("Node {}, error {}", node, e),
            }
        }
    }
}
