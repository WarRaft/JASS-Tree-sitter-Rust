use crate::lsp::semantic::TokenType;
use crate::util::uri_map::UriMapEntry;

pub fn parse(entry: UriMapEntry) {
    let tree = match entry.tree {
        &mut Some(ref t) => t,
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
