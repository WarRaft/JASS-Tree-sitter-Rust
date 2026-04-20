//! Server-side storage for the Warcraft III game installation path.
//!
//! The path is persisted to the shared `redb` database (`META_TABLE`)
//! so it survives LSP server restarts.
//!
//! Exposed via two LSP requests:
//! - `mapEditor/gamePath/set`   – update the path (writes to memory + disk)
//! - `mapEditor/gamePath/status` – query the current path and check required MPQ files

use crate::util::cache_db::{db, META_TABLE};
use log::error;
use redb::ReadableDatabase;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

// ─── Required MPQ files ──────────────────────────────────────────────────────

const REQUIRED_MPQ_FILES: &[&str] = &[
    "War3.mpq",
    "War3x.mpq",
    "War3xLocal.mpq",
    "War3Patch.mpq",
];

// ─── Persistence key ─────────────────────────────────────────────────────────

const META_KEY_GAME_PATH: &str = "game_path";

// ─── Global in-memory cache ──────────────────────────────────────────────────

/// `None` = not yet loaded from disk; `Some(s)` = loaded.
static GAME_PATH: Mutex<Option<String>> = Mutex::new(None);

/// Current map tileset letter (e.g. `"L"` for Lordaeron Summer).
/// Set when a w3e file is parsed; used by `file_lookup` to include `{tileset}.mpq`.
static TILESET: Mutex<Option<String>> = Mutex::new(None);

/// Load from the `redb` database (called once on first access).
fn load_from_db() -> String {
    let Some(database) = db() else { return String::new() };
    let Ok(read_txn) = database.begin_read() else { return String::new() };
    let Ok(table): Result<redb::ReadOnlyTable<&str, &str>, _> = read_txn.open_table(META_TABLE) else { return String::new() };
    match table.get(META_KEY_GAME_PATH) {
        Ok(Some(guard)) => {
            let val: &str = guard.value();
            val.to_string()
        }
        _ => String::new(),
    }
}

/// Write to the `redb` database.
fn save_to_db(path: &str) {
    let Some(database) = db() else { return };
    let write_txn = match database.begin_write() {
        Ok(t) => t,
        Err(e) => {
            error!("game_path: begin_write: {}", e);
            return;
        }
    };
    {
        let mut table = match write_txn.open_table(META_TABLE) {
            Ok(t) => t,
            Err(e) => {
                error!("game_path: open META_TABLE: {}", e);
                return;
            }
        };
        if let Err(e) = table.insert(META_KEY_GAME_PATH, path) {
            error!("game_path: insert: {}", e);
            return;
        }
    }
    if let Err(e) = write_txn.commit() {
        error!("game_path: commit: {}", e);
    }
}

/// Update the stored game path (memory + disk).
pub fn set_game_path(path: &str) {
    if let Ok(mut gp) = GAME_PATH.lock() {
        *gp = Some(path.to_string());
    }
    save_to_db(path);
    // Invalidate cached WorldEditStrings so they are reloaded from the new path.
    super::westrings::invalidate();
}

/// Read the current game path (lazy-loads from disk on first call).
pub fn get_game_path() -> String {
    let mut guard = match GAME_PATH.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    match &*guard {
        Some(s) => s.clone(),
        None => {
            let loaded = load_from_db();
            *guard = Some(loaded.clone());
            loaded
        }
    }
}

// ─── Tileset ─────────────────────────────────────────────────────────────────

/// Update the current tileset letter (e.g. `"L"`).
/// Called when a w3e/w3i file is parsed so that `file_lookup` can include
/// `{tileset}.mpq` in the cascade for all subsequent lookups.
pub fn set_tileset(tileset: &str) {
    if let Ok(mut guard) = TILESET.lock() {
        *guard = Some(tileset.to_string());
    }
}

/// Read the current tileset letter.  Returns `None` if not yet set.
pub fn get_tileset() -> Option<String> {
    let guard = match TILESET.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    guard.clone()
}

// ─── Tileset MPQ discovery ───────────────────────────────────────────────────

/// All known tileset letters (from docs/protocol/terrain.md).
const TILESET_LETTERS: &[char] = &[
    'A', 'B', 'C', 'D', 'F', 'G', 'I', 'J', 'K', 'L',
    'N', 'O', 'Q', 'V', 'W', 'X', 'Y', 'Z',
];

/// MPQ archives to search for nested tileset MPQs (same order as file_lookup).
const TILESET_MPQ_SEARCH_ORDER: &[&str] = &[
    "War3Patch.mpq",
    "War3xLocal.mpq",
    "War3x.mpq",
    "War3.mpq",
];

/// Discovered tileset MPQ absolute paths, keyed by uppercase letter.
/// E.g. `"Y" → "/path/to/game/Y.mpq"` or `"Y" → "/tmp/jass-tree-sitter/tileset-mpq/Y.mpq"`.
static TILESET_MPQS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Scan the game directory for per-tileset MPQ files (e.g. `Y.mpq`).
///
/// Checks two locations for each tileset letter:
/// 1. Loose file on disk (case-insensitive filename match)
/// 2. Entry inside War3*.mpq archives (extracted to a temp directory)
///
/// Stores results in [`TILESET_MPQS`].  Call during [`super::snapshot::build_snapshot`].
pub fn discover_tileset_mpqs() {
    let game_path = get_game_path();
    if game_path.is_empty() {
        log::info!("discover_tileset_mpqs: game path is empty, skipping");
        return;
    }
    let game_dir = Path::new(&game_path);

    let mut found: HashMap<String, String> = HashMap::new();

    // Read game directory entries once for case-insensitive matching.
    let dir_entries: Vec<(String, std::path::PathBuf)> = match std::fs::read_dir(game_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| {
                let lower = e.file_name().to_string_lossy().to_ascii_lowercase();
                (lower, e.path())
            })
            .collect(),
        Err(_) => Vec::new(),
    };

    for &ch in TILESET_LETTERS {
        let key = ch.to_string();
        let target_lower = format!("{}.mpq", ch.to_ascii_lowercase());

        // 1. Loose file on disk (case-insensitive).
        let mut on_disk = false;
        for (name_lower, path) in &dir_entries {
            if name_lower == &target_lower && path.is_file() {
                let abs = path.to_string_lossy().to_string();
                log::info!("discover_tileset_mpqs: {key}.mpq on disk: {abs}");
                found.insert(key.clone(), abs);
                on_disk = true;
                break;
            }
        }
        if on_disk {
            continue;
        }

        // 2. Inside War3*.mpq archives.
        let mpq_name = format!("{ch}.mpq");
        for &war3 in TILESET_MPQ_SEARCH_ORDER {
            let war3_path = game_dir.join(war3);
            if !war3_path.exists() {
                continue;
            }
            let Ok(archive) = storm_rs::MpqArchive::open(war3_path.to_string_lossy().as_ref()) else {
                continue;
            };
            let Ok(buf) = archive.read_file(&mpq_name) else {
                continue;
            };
            // Extract to a temp file so storm_rs can open it later.
            let temp_dir = std::env::temp_dir().join("jass-tree-sitter").join("tileset-mpq");
            if std::fs::create_dir_all(&temp_dir).is_err() {
                continue;
            }
            let temp_path = temp_dir.join(&mpq_name);
            if std::fs::write(&temp_path, &buf).is_ok() {
                let abs = temp_path.to_string_lossy().to_string();
                log::info!("discover_tileset_mpqs: {key}.mpq extracted from {war3} → {abs}");
                found.insert(key.clone(), abs);
            }
            break; // Found in this archive — don't check lower-priority ones.
        }
    }

    log::info!(
        "discover_tileset_mpqs: {} tileset MPQs: {:?}",
        found.len(),
        found.keys().collect::<Vec<_>>()
    );

    let mut guard = match TILESET_MPQS.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    *guard = Some(found);
}

/// Get the absolute path to a discovered tileset MPQ file.
/// Returns `None` if not yet discovered or the letter was not found.
pub fn get_tileset_mpq(tileset: &str) -> Option<String> {
    let ch = tileset.chars().next()?.to_ascii_uppercase();
    let key = ch.to_string();
    let guard = match TILESET_MPQS.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    guard.as_ref()?.get(&key).cloned()
}

// ─── Status ──────────────────────────────────────────────────────────────────

/// Response for `mapEditor/gamePath/status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GamePathStatus {
    /// The currently stored path (empty string if not set).
    pub game_path: String,
    /// `true` when `game_path` is non-empty.
    pub has_path: bool,
    /// Per-file existence check (`{ "War3.mpq": true, … }`).
    /// `None` when `game_path` is empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mpq_status: Option<HashMap<String, bool>>,
    /// `true` when all required MPQ files are present.
    pub all_present: bool,
}

/// Build the current status snapshot (blocking FS checks).
pub fn build_status() -> GamePathStatus {
    let game_path = get_game_path();
    let has_path = !game_path.is_empty();

    if !has_path {
        return GamePathStatus {
            game_path,
            has_path: false,
            mpq_status: None,
            all_present: false,
        };
    }

    let dir = Path::new(&game_path);
    let mut mpq_status = HashMap::new();
    let mut all_present = true;

    for &f in REQUIRED_MPQ_FILES {
        let exists = dir.join(f).exists();
        mpq_status.insert(f.to_string(), exists);
        if !exists {
            all_present = false;
        }
    }

    GamePathStatus {
        game_path,
        has_path: true,
        mpq_status: Some(mpq_status),
        all_present,
    }
}
