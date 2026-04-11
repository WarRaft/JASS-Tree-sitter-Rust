fn main() {
    let lang: tree_sitter::Language = tree_sitter_as::language().into();
    let count = lang.node_kind_count();
    println!("Total node kinds: {}", count);
    for i in 0..count {
        let name = lang.node_kind_for_id(i as u16).unwrap_or("???");
        let named = lang.node_kind_is_named(i as u16);
        println!("  {:>3}  named={:<5}  {}", i, named, name);
    }
    println!("\nFields:");
    let fc = lang.field_count();
    for i in 1..=fc {
        let name = lang.field_name_for_id(i as u16).unwrap_or("???");
        println!("  {:>3}  {}", i, name);
    }

    // If a file path is given, parse and dump full tree
    if let Some(path) = std::env::args().nth(1) {
        let src = std::fs::read_to_string(&path).expect("cannot read file");
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(&src, None).unwrap();
        println!("\nTree (all nodes):");
        let mut cursor = tree.root_node().walk();
        let mut depth = 0;
        loop {
            let node = cursor.node();
            let indent = "  ".repeat(depth);
            let field = cursor.field_name().unwrap_or("");
            let text: String = src[node.start_byte()..node.end_byte()].chars().take(40).collect();
            println!("{}{}{} [{}] {:?}", indent, if field.is_empty() { "".to_string() } else { format!("{}:", field) }, node.kind(), node.kind_id(), text);
            if cursor.goto_first_child() { depth += 1; continue; }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() { return; }
                depth -= 1;
            }
        }
    }
}
