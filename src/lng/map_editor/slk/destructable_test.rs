#[cfg(test)]
mod tests {
    use crate::lng::map_editor::slk::*;

    #[test]
    fn parse_destructable_slk_fixture() {
        let data = include_bytes!("../../../lng/slk/fixtures/Units/DestructableData.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some rows");

        let first = &rows[0];
        assert_eq!(first.get("DestructableID").map(|s| s.as_str()), Some("ATtr"));
        assert_eq!(first.get("comment").map(|s| s.as_str()), Some("Tree Wall"));
        assert_eq!(first.get("category").map(|s| s.as_str()), Some("D"));
    }

    /// Collect all unique `category` values, tileset characters, and destructable
    /// names from `Units\DestructableData.slk` so we can use them for UI filters.
    ///
    /// Run manually:
    /// ```sh
    /// cargo test --package JASS-Tree-sitter-Rust map_editor::slk::destructable::tests::dump_destructable_categories_and_tilesets -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_destructable_categories_and_tilesets() {
        use std::collections::BTreeMap;

        let data = include_bytes!("../../../lng/slk/fixtures/Units/DestructableData.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some destructable rows");

        let mut categories: BTreeMap<String, usize> = BTreeMap::new();
        let mut tilesets: BTreeMap<char, usize> = BTreeMap::new();
        let mut names: Vec<(String, String, String, String)> = Vec::new();

        for row in &rows {
            let dest_id = row.get("DestructableID").cloned().unwrap_or_default();
            if dest_id.is_empty() {
                continue;
            }

            let cat = row.get("category").cloned().unwrap_or_default();
            if !cat.is_empty() {
                *categories.entry(cat.clone()).or_insert(0) += 1;
            }

            let ts = row.get("tilesets").cloned().unwrap_or_default();
            for ch in ts.chars() {
                *tilesets.entry(ch).or_insert(0) += 1;
            }

            let name = row.get("Name").cloned().unwrap_or_default();
            names.push((dest_id, name, cat, ts));
        }

        println!("\n══════════════════════════════════════════");
        println!("  DestructableData.slk — {} entries", rows.len());
        println!("══════════════════════════════════════════\n");

        println!("── Categories ({}) ──", categories.len());
        for (cat, count) in &categories {
            println!("  {:<20} {:>4} destructables", cat, count);
        }

        println!("\n── Tileset characters ({}) ──", tilesets.len());
        for (ch, count) in &tilesets {
            println!("  '{}' {:>5} destructables", ch, count);
        }

        println!("\n── Destructable names (first 30) ──");
        for (id, name, cat, ts) in names.iter().take(30) {
            println!("  {} | {:<40} | cat={:<16} | ts={}", id, name, cat, ts);
        }

        println!("\nTotal categories: {}", categories.len());
        println!("Total tileset chars: {}", tilesets.len());
        println!("Total destructable entries: {}", rows.len());
    }

    /// Dump all column names from `DestructableData.slk` into a text file next to the fixture.
    ///
    /// Run manually:
    /// ```sh
    /// cargo test --package JASS-Tree-sitter-Rust map_editor::slk::destructable::tests::dump_destructable_field_names -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_destructable_field_names() {
        use std::collections::BTreeSet;

        let data = include_bytes!("../../../lng/slk/fixtures/Units/DestructableData.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some destructable rows");

        let mut fields = BTreeSet::new();
        for row in &rows {
            for key in row.keys() {
                fields.insert(key.clone());
            }
        }

        let out_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/lng/slk/fixtures/Units/destructable_field_names.txt"
        );
        let content = fields.iter().cloned().collect::<Vec<_>>().join("\n");
        std::fs::write(out_path, &content).expect("failed to write destructable_field_names.txt");

        println!("\nWrote {} field names to {}", fields.len(), out_path);
        for f in &fields {
            println!("  {}", f);
        }
    }
}

