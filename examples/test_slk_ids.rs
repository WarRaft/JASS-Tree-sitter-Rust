use tree_sitter::Parser;
fn main() {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_slk::LANGUAGE.into()).unwrap();
    let source = "ID;PWXL;N;E\nB;X1;Y1;D0\nC;X1;Y1;K\"test\"\nE\n";
    let tree = parser.parse(source, None).unwrap();
    fn dump(node: tree_sitter::Node, source: &str, depth: usize) {
        let indent = "  ".repeat(depth);
        let text = &source[node.start_byte()..node.end_byte()];
        let text_short = if text.len() > 30 { &text[..30] } else { text };
        println!("{}{} grammar_id={} kind_id={} named={} text={:?}", 
            indent, node.kind(), node.grammar_id(), node.kind_id(), node.is_named(), text_short);
        for i in 0..node.child_count() as u32 {
            if let Some(child) = node.child(i) {
                dump(child, source, depth + 1);
            }
        }
    }
    dump(tree.root_node(), source, 0);
}
