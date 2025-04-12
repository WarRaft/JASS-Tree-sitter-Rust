use tree_sitter::{Node, Parser};
use tree_sitter_lua::LANGUAGE;
fn main() {
    let mut parser = Parser::new();
    let language = LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("Error loading Vjass parser");

    let source_code = r#"[1, 2, {{"a": 3, "b": 4}]"#;

    let tree = parser.parse(source_code, None).unwrap();

    let root_node = tree.root_node();

    println!("Tree structure:");
    print_node(root_node, source_code, 0);

    println!("{:?}", tree.root_node().to_sexp());
}

fn print_node(node: Node, source: &str, indent: usize) {
    let indent_str = "  ".repeat(indent);
    let kind = node.kind();
    let text = node.utf8_text(source.as_bytes()).unwrap_or("<?>");
    let pos = node.start_position();

    println!(
        "{indent_str}{kind} (line {}, column {}): {}",
        pos.row,
        pos.column,
        text.replace("\n", "\\n")
    );

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            print_node(child, source, indent + 1);
        }
    }
}
