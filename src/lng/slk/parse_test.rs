#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use tree_sitter::Parser;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("lng")
            .join("slk")
            .join("fixtures")
    }

    fn collect_slk_files(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if !dir.is_dir() {
            return files;
        }
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_slk_files(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("slk") {
                files.push(path);
            }
        }
        files.sort();
        files
    }

    /// Parse every .slk fixture with tree-sitter and assert zero errors/missing nodes.
    ///
    /// Run manually:
    /// ```sh
    /// cargo test --package JASS-Tree-sitter-Rust slk::parse_test -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn parse_all_fixtures() {
        let dir = fixtures_dir();
        let files = collect_slk_files(&dir);
        assert!(!files.is_empty(), "No .slk fixtures found in {}", dir.display());

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_slk::LANGUAGE.into())
            .expect("Failed to set tree-sitter-slk language");

        let mut failed: Vec<String> = Vec::new();

        for path in &files {
            let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
                panic!("Cannot read {}: {e}", path.display());
            });

            let tree = parser.parse(&text, None).unwrap_or_else(|| {
                panic!("tree-sitter returned None for {}", path.display());
            });

            let root = tree.root_node();
            let rel = path.strip_prefix(&dir).unwrap_or(path);

            // Collect errors
            let mut errors: Vec<String> = Vec::new();
            let mut cursor = root.walk();
            let mut reached = true;
            while reached {
                let node = cursor.node();
                if node.is_error() {
                    errors.push(format!(
                        "  ERROR at {}:{} – {}..{}: {:?}",
                        node.start_position().row + 1,
                        node.start_position().column,
                        node.start_byte(),
                        node.end_byte(),
                        &text[node.start_byte()..node.end_byte().min(node.start_byte() + 60)],
                    ));
                }
                if node.is_missing() {
                    errors.push(format!(
                        "  MISSING '{}' at {}:{}",
                        node.kind(),
                        node.start_position().row + 1,
                        node.start_position().column,
                    ));
                }
                // DFS
                if cursor.goto_first_child() {
                    continue;
                }
                while !cursor.goto_next_sibling() {
                    if !cursor.goto_parent() {
                        reached = false;
                        break;
                    }
                }
            }

            if !errors.is_empty() {
                failed.push(format!("{}:\n{}", rel.display(), errors.join("\n")));
            }
        }

        if failed.is_empty() {
            println!("All {} .slk fixtures parsed without errors.", files.len());
        } else {
            panic!(
                "{} of {} .slk fixtures have parse errors:\n\n{}",
                failed.len(),
                files.len(),
                failed.join("\n\n")
            );
        }
    }
}

