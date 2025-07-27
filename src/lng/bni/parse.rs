use crate::lng::bni::kind::Kind;
use crate::lsp::semantic::TokenType;
use crate::util::uri_map::UriMapEntry;
use log::error;

pub fn parse(entry: UriMapEntry) {
    let tree = match entry.tree {
        &mut Some(ref t) => t,
        None => return,
    };

    let root = tree.root_node();
    let semantic = entry.semantic.clear();

    for i in 0..root.child_count() {
        let node = root.child(i).unwrap();
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
