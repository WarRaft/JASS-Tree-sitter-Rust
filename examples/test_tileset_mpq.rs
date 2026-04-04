/// Quick diagnostic: check if `Y.mpq` (Cityscape) at the game path contains
/// `ReplaceableTextures\Cliff\Cliff1.blp`.
///
/// Run: `cargo run --example test_tileset_mpq`

fn main() {
    use redb::ReadableDatabase;

    // Try to read the game path from the redb database (same as the LSP server uses).
    let db_path = dirs::data_local_dir()
        .map(|d| d.join("JASS-Tree-sitter-Rust").join("cache.redb"))
        .unwrap_or_default();

    println!("DB path: {}", db_path.display());

    let game_path = if db_path.exists() {
        match redb::Database::open(&db_path) {
            Ok(database) => {
                let table_def: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("meta");
                match database.begin_read() {
                    Ok(txn) => {
                        let table: redb::ReadOnlyTable<&str, &str> = match txn.open_table(table_def) {
                            Ok(t) => t,
                            Err(e) => { eprintln!("Cannot open table: {e}"); return; }
                        };
                        match table.get("game_path") {
                            Ok(Some(guard)) => {
                                let v: &str = guard.value();
                                v.to_string()
                            }
                            _ => String::new(),
                        }
                    },
                    Err(e) => { eprintln!("Cannot begin read: {e}"); String::new() }
                }
            }
            Err(e) => { eprintln!("Cannot open DB: {e}"); String::new() }
        }
    } else {
        eprintln!("DB file not found at {}", db_path.display());
        String::new()
    };

    if game_path.is_empty() {
        // Fallback: try env var
        let gp = std::env::var("GAME_PATH").unwrap_or_default();
        if gp.is_empty() {
            eprintln!("Game path not set. Use: GAME_PATH=/path/to/game cargo run --example test_tileset_mpq");
            return;
        }
        run_test(&gp);
    } else {
        println!("Game path from DB: {game_path}");
        run_test(&game_path);
    }
}

fn run_test(game_path: &str) {
    let game_dir = std::path::Path::new(game_path);
    println!("\n=== Game directory: {} ===", game_dir.display());
    println!("Exists: {}", game_dir.exists());
    println!();

    // List all .mpq files in the game directory
    println!("MPQ files in game directory:");
    if let Ok(entries) = std::fs::read_dir(game_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.to_ascii_lowercase().ends_with(".mpq") {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                println!("  {} ({} bytes)", name, size);
            }
        }
    }
    println!();

    let test_file = r"ReplaceableTextures\Cliff\Cliff1.blp";

    // Test Y.mpq specifically
    for tileset in ["Y", "L", "A"] {
        let mpq_name = format!("{tileset}.mpq");
        let mpq_path = game_dir.join(&mpq_name);
        println!("--- Testing {mpq_name} ---");
        println!("  Path: {}", mpq_path.display());
        println!("  Exists: {}", mpq_path.exists());

        if !mpq_path.exists() {
            println!("  SKIPPED (not found)");
            println!();
            continue;
        }

        match storm_rs::MpqArchive::open(mpq_path.to_string_lossy().as_ref()) {
            Ok(archive) => {
                println!("  Opened OK");
                match archive.read_file(test_file) {
                    Ok(buf) => println!("  FOUND: {} ({} bytes)", test_file, buf.len()),
                    Err(e) => println!("  NOT FOUND: {} (error: {e})", test_file),
                }
            }
            Err(e) => println!("  OPEN FAILED: {e}"),
        }
        println!();
    }

    // Also test War3.mpq for comparison
    let war3_path = game_dir.join("War3.mpq");
    println!("--- Testing War3.mpq ---");
    if war3_path.exists() {
        match storm_rs::MpqArchive::open(war3_path.to_string_lossy().as_ref()) {
            Ok(archive) => {
                match archive.read_file(test_file) {
                    Ok(buf) => println!("  FOUND: {} ({} bytes)", test_file, buf.len()),
                    Err(e) => println!("  NOT FOUND: {} (error: {e})", test_file),
                }
            }
            Err(e) => println!("  OPEN FAILED: {e}"),
        }
    } else {
        println!("  NOT FOUND");
    }
}

