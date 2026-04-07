#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use tree_sitter::Parser;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("lng")
            .join("bni")
            .join("fixtures")
    }

    fn collect_txt_files(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if !dir.is_dir() {
            return files;
        }
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_txt_files(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("txt") {
                files.push(path);
            }
        }
        files.sort();
        files
    }

    /// Parse every .txt fixture with tree-sitter-bni and assert zero errors/missing nodes.
    ///
    /// Run manually:
    /// ```sh
    /// cargo test --package JASS-Tree-sitter-Rust bni::parse_test -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn parse_all_fixtures() {
        let dir = fixtures_dir();
        let files = collect_txt_files(&dir);
        assert!(!files.is_empty(), "No .txt fixtures found in {}", dir.display());

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_bni::LANGUAGE.into())
            .expect("Failed to set tree-sitter-bni language");

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
            println!("All {} .txt fixtures parsed without errors.", files.len());
        } else {
            panic!(
                "{} of {} .txt fixtures have parse errors:\n\n{}",
                failed.len(),
                files.len(),
                failed.join("\n\n")
            );
        }
    }

    /// Extract all .txt files from Warcraft III MPQ archives into fixtures/.
    ///
    /// Requires a valid game path set via `GAME_PATH` env var.
    ///
    /// Run manually:
    /// ```sh
    /// GAME_PATH="/path/to/warcraft3" cargo test --package JASS-Tree-sitter-Rust bni::parse_test::tests::extract_txt_fixtures -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn extract_txt_fixtures() {
        let game_path = std::env::var("GAME_PATH").unwrap_or_else(|_| {
            // Fallback: try the global game path setting
            crate::lng::map_editor::game_path::get_game_path()
        });
        assert!(
            !game_path.is_empty(),
            "Set GAME_PATH env var or configure game path in the extension first"
        );

        let game_dir = std::path::Path::new(&game_path);
        assert!(game_dir.is_dir(), "Game path does not exist: {game_path}");

        let listfile = include_str!("../../../listfile.txt");
        let txt_paths: Vec<&str> = listfile
            .lines()
            .filter(|line| {
                let lower = line.to_ascii_lowercase();
                lower.ends_with(".txt")
                    && !lower.starts_with("custom_v")
                    && !lower.starts_with("melee_v")
            })
            .collect();

        assert!(
            !txt_paths.is_empty(),
            "No .txt entries found in listfile.txt"
        );

        let mpq_names = &[
            "War3Patch.mpq",
            "War3xLocal.mpq",
            "War3x.mpq",
            "War3.mpq",
        ];

        let out_dir = fixtures_dir();
        std::fs::create_dir_all(&out_dir).expect("Failed to create fixtures dir");

        let mut extracted = 0u32;
        let mut skipped = 0u32;

        for &rel_path in &txt_paths {
            let mpq_path = rel_path.replace('/', "\\");

            let mut found = false;

            // Try each MPQ in priority order
            for &mpq_name in mpq_names {
                let mpq_file = game_dir.join(mpq_name);
                if !mpq_file.exists() {
                    continue;
                }
                let archive = match storm_rs::MpqArchive::open(mpq_file.to_string_lossy().as_ref()) {
                    Ok(a) => a,
                    Err(_) => continue,
                };
                let buf = match archive.read_file(&mpq_path) {
                    Ok(b) => b,
                    Err(_) => continue,
                };

                // Preserve directory structure under fixtures/
                let fs_rel = rel_path.replace('\\', "/");
                let dest = out_dir.join(&fs_rel);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&dest, &buf).unwrap();
                println!("  \u{2714} {} ({} bytes, from {})", fs_rel, buf.len(), mpq_name);
                extracted += 1;
                found = true;
                break; // found in first (highest priority) MPQ
            }

            if !found {
                println!("  \u{2718} {} — not found in any MPQ", rel_path);
                skipped += 1;
            }
        }

        println!(
            "\nExtracted {} files, skipped {} (to {})",
            extracted,
            skipped,
            out_dir.display()
        );
    }
}

