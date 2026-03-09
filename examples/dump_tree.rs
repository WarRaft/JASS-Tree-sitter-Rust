use tree_sitter::Node;

fn print_tree(node: Node, src: &str, indent: usize) {
    let pad = " ".repeat(indent);
    let text = if node.child_count() == 0 {
        format!(" {:?}", &src[node.start_byte()..node.end_byte()])
    } else {
        String::new()
    };
    let field = node.parent().and_then(|p| {
        for i in 0..p.child_count() {
            if let Some(c) = p.child(i as u32) {
                if c.id() == node.id() {
                    return p.field_name_for_child(i as u32).map(|s| format!("{}: ", s));
                }
            }
        }
        None
    }).unwrap_or_default();
    println!(
        "{}{}{} [{}..{}] kind_id={} grammar_id={}{}",
        pad, field, node.kind(), node.start_byte(), node.end_byte(),
        node.kind_id(), node.grammar_id(), text
    );
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i as u32) {
            print_tree(c, src, indent + 2);
        }
    }
}

fn main() {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_jass::language().into())
        .unwrap();

    let src = r#"call Foo("my shit")
"#;

    let tree = parser.parse(src, None).unwrap();
    print_tree(tree.root_node(), src, 0);
}

