use log::info;
use tree_sitter::Parser;

pub fn parse(text: impl AsRef<[u8]>){
    let language = tree_sitter_bni::LANGUAGE;
    let mut parser = Parser::new();

    parser
        .set_language(&language.into())
        .expect("Error loading Bni parser");

    let tree = parser.parse(&text, None).unwrap();

    let root = tree.root_node();

    for i in 0..root.child_count() {
        let node = root.child(i).unwrap();
        info!(
                                "{:?}|s:{:?}|e:{:?}",
                                node.kind(),
                                node.start_position(),
                                node.end_position()
                            );
    }

}