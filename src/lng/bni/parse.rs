use crate::lng::bni::kind::Kind;
use crate::lsp::semantic::Kind as TokenKind;
use crate::lsp::semantic_hub::SemanticTokenHub;
use crate::util::node_kinded::NodeKindedExt;
use crate::util::uri_map::{SEMANTIC_MAP, TREE_MAP};
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

    for (kind, node) in tree.root_node().kinds::<Kind>() {
        match kind {
            Kind::Section => {
                for (sec_kind, sec_node) in node.kinds::<Kind>() {
                    match sec_kind {
                        Kind::LeftBracket | Kind::RightBracket => {
                            semantic.add_node(&sec_node, TokenKind::Comment, 0u32);
                        }

                        Kind::SectionName => {
                            semantic.add_node(&sec_node, TokenKind::Keyword, 0u32);
                        }
                        _ => {}
                    }
                }
            }
            Kind::Item => {
                for (item_kind, item) in node.kinds::<Kind>() {
                    if item_kind == Kind::Key {
                        semantic.add_node(&item, TokenKind::Function, 0u32);
                    }
                }
            }
            Kind::Comment => {
                semantic.add_node(&node, TokenKind::Comment, 0u32);
            }
            _ => {}
        }
    }
}
