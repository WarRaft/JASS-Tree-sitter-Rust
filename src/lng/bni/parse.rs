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
                            semantic.add_node(&sec_node, TokenKind::Operator, 0u32);
                        }

                        Kind::SectionName => {
                            semantic.add_node(&sec_node, TokenKind::Keyword, 0u32);
                        }
                        _ => {}
                    }
                }
            }
            Kind::Item => {
                for (item_kind, item_node) in node.kinds::<Kind>() {
                    match item_kind {
                        Kind::Key => {
                            semantic.add_node(&item_node, TokenKind::Function, 0u32);
                        }
                        Kind::Equal => {
                            semantic.add_node(&item_node, TokenKind::Operator, 0u32);
                        }
                        Kind::ValueList => {
                            for (val_kind, val_node) in item_node.kinds::<Kind>() {
                                match val_kind {
                                    Kind::QuotedString | Kind::UnquotedString => {
                                        semantic.add_node(&val_node, TokenKind::String, 0u32);
                                    }
                                    Kind::Int => {
                                        semantic.add_node(&val_node, TokenKind::Number, 0u32);
                                    }
                                    Kind::Comma => {
                                        semantic.add_node(&val_node, TokenKind::Operator, 0u32);
                                    }
                                    _ => {}
                                }
                            }
                        }

                        _ => {}
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
