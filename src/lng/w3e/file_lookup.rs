//! Cascading file lookup for Warcraft III data files.
//!
//! Search order:
//! 1. The map archive itself (if `archive_path` is provided)
//! 2. The game folder directly (file on disk)
//! 3. `War3Patch.mpq`
//! 4. `War3xLocal.mpq`
//! 5. `War3x.mpq`
//! 6. `War3.mpq`

use crate::lng::w3e::game_path::get_game_path;
use log::debug;
use std::path::Path;

/// MPQ archives to search, in priority order.
const MPQ_SEARCH_ORDER: &[&str] = &[
    "War3Patch.mpq",
    "War3xLocal.mpq",
    "War3x.mpq",
    "War3.mpq",
];

/// Try to find `relative_path` (e.g. `"TerrainArt\Terrain.slk"`) using the
/// cascading lookup.  Returns `(file_bytes, source_label)` on success.
pub fn lookup_file(relative_path: &str, archive_path: Option<&str>) -> Option<(Vec<u8>, String)> {
    // Normalise to backslash for MPQ lookups and forward-slash for FS.
    let mpq_path = relative_path.replace('/', "\\");
    let fs_path = relative_path.replace('\\', "/");

    // ── 1. Map archive ───────────────────────────────────────────
    if let Some(ap) = archive_path {
        if let Ok(archive) = storm_rs::MpqArchive::open(ap) {
            if let Ok(buf) = archive.read_file(&mpq_path) {
                debug!("lookup_file: found {relative_path} in map archive");
                return Some((buf, "map archive".into()));
            }
        }
    }

    // ── 2–6. Game folder + MPQ chain ─────────────────────────────
    let game_path = get_game_path();
    if game_path.is_empty() {
        return None;
    }
    let game_dir = Path::new(&game_path);

    // 2. Direct file on disk
    let disk_path = game_dir.join(&fs_path);
    if disk_path.is_file() {
        if let Ok(buf) = std::fs::read(&disk_path) {
            debug!("lookup_file: found {relative_path} on disk");
            return Some((buf, "game folder".into()));
        }
    }

    // 3–6. MPQ archives
    for &mpq_name in MPQ_SEARCH_ORDER {
        let mpq_file = game_dir.join(mpq_name);
        if !mpq_file.exists() {
            continue;
        }
        if let Ok(archive) = storm_rs::MpqArchive::open(mpq_file.to_string_lossy().as_ref()) {
            if let Ok(buf) = archive.read_file(&mpq_path) {
                debug!("lookup_file: found {relative_path} in {mpq_name}");
                return Some((buf, mpq_name.into()));
            }
        }
    }

    None
}

/// Check whether `relative_path` exists anywhere in the cascade (without
/// reading the contents).  Returns `true` on first hit.
pub fn lookup_file_exists(relative_path: &str, archive_path: Option<&str>) -> bool {
    let mpq_path = relative_path.replace('/', "\\");
    let fs_path = relative_path.replace('\\', "/");

    // 1. Map archive
    if let Some(ap) = archive_path {
        if let Ok(archive) = storm_rs::MpqArchive::open(ap) {
            if archive.read_file(&mpq_path).is_ok() {
                return true;
            }
        }
    }

    // 2–6. Game folder + MPQ chain
    let game_path = get_game_path();
    if game_path.is_empty() {
        return false;
    }
    let game_dir = Path::new(&game_path);

    // 2. Direct file on disk
    if game_dir.join(&fs_path).is_file() {
        return true;
    }

    // 3–6. MPQ archives
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

    false
}

