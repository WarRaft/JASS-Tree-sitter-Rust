use crate::lng::bni::kind::Kind;
use crate::lsp::semantic::Kind as TokenKind;
use crate::lsp::semantic_hub::SemanticTokenHub;
use crate::util::uri_map::{SEMANTIC_MAP, TREE_MAP};
use log::error;
use tree_sitter::Node;
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

    fn walk(
        node: Node,
        semantic: &mut SemanticTokenHub,
        current_section: Option<Node>,
        current_item: Option<Node>,
    ) {
        if node.is_missing() {
            return;
        }

        if node.is_error() || node.has_error() {
            //semantic.add_node(&node, TokenKind::Invalid, 0u32);
        }

        let mut new_section = None;
        let mut new_item = None;

        if !node.is_error() {
            let kind = match Kind::try_from(node.grammar_id()) {
                Ok(k) => k,
                Err(_) => {
                    error!("Unkown error kind {:?}", node.kind());
                    return;
                }
            };

            new_section = match kind {
                Kind::Section => Some(node),
                _ => current_section,
            };

            new_item = match kind {
                Kind::Item => Some(node),
                Kind::Section | Kind::Comment => None,
                _ => current_item,
            };

            match kind {
                Kind::LeftBracket | Kind::RightBracket | Kind::Equal | Kind::Comma => {
                    semantic.add_node(&node, TokenKind::Operator, 0u32);
                }
                Kind::SectionName => {
                    semantic.add_node(&node, TokenKind::Keyword, 0u32);
                }
                Kind::Key => {
                    semantic.add_node(&node, TokenKind::Function, 0u32);
                }
                Kind::QuotedString | Kind::UnquotedString => {
                    semantic.add_node(&node, TokenKind::String, 0u32);
                }
                Kind::Int | Kind::Float => {
                    semantic.add_node(&node, TokenKind::Number, 0u32);
                }
                Kind::LineComment => {
                    semantic.add_node(&node, TokenKind::Comment, 0u32);
                }
                _ => {}
            }
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                walk(child, semantic, new_section, new_item);
            }
        }
    }

    let root = tree.root_node();
    for i in 0..root.child_count() {
        if let Some(child) = root.child(i) {
            walk(child, semantic, None, None);
        }
    }
}
