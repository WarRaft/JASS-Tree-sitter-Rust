fn main() {
    let lang: tree_sitter::Language = tree_sitter_as::language().into();
    let count = lang.node_kind_count();
    println!("Total node kinds: {}", count);
    for i in 0..count {
        let name = lang.node_kind_for_id(i as u16).unwrap_or("???");
        let named = lang.node_kind_is_named(i as u16);
        println!("  {:>3} | named={:<5} | {}", i, named, name);
    }
    println!("\nFields:");
    let fc = lang.field_count();
    for i in 1..=fc {
        let name = lang.field_name_for_id(i as u16).unwrap_or("???");
        println!("  {:>3} | {}", i, name);
    }
}
