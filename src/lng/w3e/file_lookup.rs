//! Cascading file lookup for Warcraft III data files.
//!
//! Search order:
//! 1a. The map archive itself (if `archive_path` is provided)
//! 1b. `{tileset}.mpq` inside the map archive (extracted to temp)
//! 2.  `{tileset}.mpq` from the game (pre-discovered: loose file or extracted from War3*.mpq)
//! 3.  The game folder directly (file on disk)
//! 4.  `War3Patch.mpq`
//! 5.  `War3xLocal.mpq`
//! 6.  `War3x.mpq`
//! 7.  `War3.mpq`

use crate::lng::w3e::game_path::get_game_path;
use log::debug;
use std::path::Path;

/// MPQ archives to search (after the tileset MPQ).
const MPQ_SEARCH_ORDER: &[&str] = &[
    "War3Patch.mpq",
    "War3xLocal.mpq",
    "War3x.mpq",
    "War3.mpq",
];

/// Try to find `relative_path` (e.g. `"TerrainArt\Terrain.slk"`) using the
/// cascading lookup.  Returns `(file_bytes, source_label)` on success.
/// Automatically includes the global tileset MPQ (set via `set_tileset`).
pub fn lookup_file(relative_path: &str, archive_path: Option<&str>) -> Option<(Vec<u8>, String)> {
    let ts = super::game_path::get_tileset();
    lookup_file_ext(relative_path, archive_path, ts.as_deref())
}

/// Like `lookup_file`, but with an explicit tileset override.
pub fn lookup_file_ext(relative_path: &str, archive_path: Option<&str>, tileset: Option<&str>) -> Option<(Vec<u8>, String)> {
    lookup_file_resolved_ext(relative_path, archive_path, tileset)
        .map(|(buf, source, _resolved)| (buf, source))
}

/// Like `lookup_file`, but also returns the actual path that matched
/// (may differ from `relative_path` when `.mdx` → `.mdl` fallback triggers).
/// Automatically includes the global tileset MPQ.
pub fn lookup_file_resolved(relative_path: &str, archive_path: Option<&str>) -> Option<(Vec<u8>, String, String)> {
    let ts = super::game_path::get_tileset();
    lookup_file_resolved_ext(relative_path, archive_path, ts.as_deref())
}

/// Like `lookup_file_resolved`, but with an optional tileset for tileset-specific MPQ lookup.
pub fn lookup_file_resolved_ext(relative_path: &str, archive_path: Option<&str>, tileset: Option<&str>) -> Option<(Vec<u8>, String, String)> {
    // Normalise to backslash for MPQ lookups and forward-slash for FS.
    let mpq_path = relative_path.replace('/', "\\");
    let fs_path = relative_path.replace('\\', "/");

    // ── 1. Map archive ───────────────────────────────────────────
    if let Some(ap) = archive_path {
        if let Ok(archive) = storm_rs::MpqArchive::open(ap) {
            // 1a. Direct file in map archive.
            if let Ok(buf) = archive.read_file(&mpq_path) {
                debug!("lookup_file: FOUND {relative_path} in map archive");
                let label = format!("{{MAP}}\\{}", mpq_path);
                return Some((buf, label, relative_path.into()));
            }

            // 1b. Tileset MPQ inside map archive — extract, open, search.
            if let Some(ts) = tileset {
                if !ts.is_empty() {
                    let ch = ts.chars().next().unwrap_or('_').to_ascii_uppercase();
                    let ts_mpq = format!("{ch}.mpq");
                    if let Ok(ts_buf) = archive.read_file(&ts_mpq) {
                        let temp_dir = std::env::temp_dir().join("jass-tree-sitter").join("map-tileset");
                        if std::fs::create_dir_all(&temp_dir).is_ok() {
                            let temp_path = temp_dir.join(&ts_mpq);
                            if std::fs::write(&temp_path, &ts_buf).is_ok() {
                                if let Ok(ts_archive) = storm_rs::MpqArchive::open(temp_path.to_string_lossy().as_ref()) {
                                    if let Ok(buf) = ts_archive.read_file(&mpq_path) {
                                        debug!("lookup_file: FOUND {relative_path} in map's {ts_mpq}");
                                        let label = format!("{{MAP}}\\{}\\{}", ts_mpq, mpq_path);
                                        return Some((buf, label, relative_path.into()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ── 2–7. Game folder + MPQ chain ─────────────────────────────
    let game_path = get_game_path();
    if game_path.is_empty() {
        debug!("lookup_file: game path is empty, cannot search for {relative_path}");
        return None;
    }
    let game_dir = Path::new(&game_path);

    // 2. {tileset}.mpq (pre-discovered: loose file on disk or extracted from War3*.mpq)
    if let Some(ts) = tileset {
        if !ts.is_empty() {
            let ch = ts.chars().next().unwrap_or('_').to_ascii_uppercase();
            let ts_mpq = format!("{ch}.mpq");
            if let Some(ts_path) = super::game_path::get_tileset_mpq(ts) {
                debug!("lookup_file: trying {relative_path} in {ts_mpq} ({ts_path})");
                if let Ok(archive) = storm_rs::MpqArchive::open(&ts_path) {
                    if let Ok(buf) = archive.read_file(&mpq_path) {
                        debug!("lookup_file: FOUND {relative_path} in {ts_mpq}");
                        let label = format!("{{GAME}}\\{}\\{}", ts_mpq, mpq_path);
                        return Some((buf, label, relative_path.into()));
                    }
                }
                debug!("lookup_file: {relative_path} NOT in {ts_mpq}");
            }
        }
    }

    // 3. Direct file on disk
    let disk_path = game_dir.join(&fs_path);
    debug!("lookup_file: trying {relative_path} on disk at {}", disk_path.display());
    if disk_path.is_file() {
        if let Ok(buf) = std::fs::read(&disk_path) {
            debug!("lookup_file: FOUND {relative_path} on disk");
            let label = format!("{{GAME}}\\{}", mpq_path);
            return Some((buf, label, relative_path.into()));
        }
    }

    // 4–7. MPQ archives: standard chain (tileset already checked above)
    for &mpq_name in MPQ_SEARCH_ORDER {
        let mpq_file = game_dir.join(mpq_name);
        if !mpq_file.exists() {
            debug!("lookup_file: skipping {mpq_name} (not found at {})", mpq_file.display());
            continue;
        }
        debug!("lookup_file: trying {relative_path} in {mpq_name}");
        if let Ok(archive) = storm_rs::MpqArchive::open(mpq_file.to_string_lossy().as_ref()) {
            if let Ok(buf) = archive.read_file(&mpq_path) {
                debug!("lookup_file: FOUND {relative_path} in {mpq_name}");
                let label = format!("{{GAME}}\\{}\\{}", mpq_name, mpq_path);
                return Some((buf, label, relative_path.into()));
            }
        }
        debug!("lookup_file: {relative_path} NOT in {mpq_name}");
    }

    // ── .mdx → .mdl fallback ────────────────────────────────────
    // Model paths in SLK files often omit the extension; callers append
    // `.mdx` first.  If the `.mdx` wasn't found anywhere in the cascade,
    // retry with `.mdl` (the older text-based model format).
    if relative_path.to_ascii_lowercase().ends_with(".mdx") {
        let mdl_path = format!("{}.mdl", &relative_path[..relative_path.len() - 4]);
        debug!("lookup_file: .mdx not found, retrying as {mdl_path}");
        return lookup_file_resolved_ext(&mdl_path, archive_path, tileset);
    }

    None
}

/// Check whether `relative_path` exists anywhere in the cascade (without
/// reading the contents).  Returns `true` on first hit.
/// Automatically includes the global tileset MPQ.
pub fn lookup_file_exists(relative_path: &str, archive_path: Option<&str>) -> bool {
    let ts = super::game_path::get_tileset();
    lookup_file_exists_ext(relative_path, archive_path, ts.as_deref())
}

/// Like `lookup_file_exists`, but with an optional tileset.
pub fn lookup_file_exists_ext(relative_path: &str, archive_path: Option<&str>, tileset: Option<&str>) -> bool {
    let mpq_path = relative_path.replace('/', "\\");
    let fs_path = relative_path.replace('\\', "/");

    // 1. Map archive
    if let Some(ap) = archive_path {
        if let Ok(archive) = storm_rs::MpqArchive::open(ap) {
            // 1a. Direct file in map archive.
            if archive.read_file(&mpq_path).is_ok() {
                return true;
            }

            // 1b. Tileset MPQ inside map archive.
            if let Some(ts) = tileset {
                if !ts.is_empty() {
                    let ch = ts.chars().next().unwrap_or('_').to_ascii_uppercase();
                    let ts_mpq = format!("{ch}.mpq");
                    if let Ok(ts_buf) = archive.read_file(&ts_mpq) {
                        let temp_dir = std::env::temp_dir().join("jass-tree-sitter").join("map-tileset");
                        if std::fs::create_dir_all(&temp_dir).is_ok() {
                            let temp_path = temp_dir.join(&ts_mpq);
                            if std::fs::write(&temp_path, &ts_buf).is_ok() {
                                if let Ok(ts_archive) = storm_rs::MpqArchive::open(temp_path.to_string_lossy().as_ref()) {
                                    if ts_archive.read_file(&mpq_path).is_ok() {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2–7. Game folder + MPQ chain
    let game_path = get_game_path();
    if game_path.is_empty() {
        return false;
    }
    let game_dir = Path::new(&game_path);

    // 2. {tileset}.mpq (pre-discovered)
    if let Some(ts) = tileset {
        if !ts.is_empty() {
            if let Some(ts_path) = super::game_path::get_tileset_mpq(ts) {
                if let Ok(archive) = storm_rs::MpqArchive::open(&ts_path) {
                    if archive.read_file(&mpq_path).is_ok() {
                        return true;
                    }
                }
            }
        }
    }

    // 3. Direct file on disk
    if game_dir.join(&fs_path).is_file() {
        return true;
    }

    // 4–7. MPQ archives: standard chain (tileset already checked above)
    for &mpq_name in MPQ_SEARCH_ORDER {
        let mpq_file = game_dir.join(mpq_name);
        if !mpq_file.exists() {
            continue;
        }
        if let Ok(archive) = storm_rs::MpqArchive::open(mpq_file.to_string_lossy().as_ref()) {
            if archive.read_file(&mpq_path).is_ok() {
                return true;
            }
        }
    }

    // ── .mdx → .mdl fallback ────────────────────────────────────
    if relative_path.to_ascii_lowercase().ends_with(".mdx") {
        let mdl_path = format!("{}.mdl", &relative_path[..relative_path.len() - 4]);
        return lookup_file_exists_ext(&mdl_path, archive_path, tileset);
    }

    false
}

