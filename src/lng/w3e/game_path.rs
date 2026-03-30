//! Server-side storage for the Warcraft III game installation path.
//!
//! The path is persisted to the shared `redb` database (`META_TABLE`)
//! so it survives LSP server restarts.
//!
//! Exposed via two LSP requests:
//! - `w3e/gamePath/set`   – update the path (writes to memory + disk)
//! - `w3e/gamePath/status` – query the current path and check required MPQ files

use crate::util::cache_db::{db, META_TABLE};
use log::error;
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

/// Load from the `redb` database (called once on first access).
fn load_from_db() -> String {
    let Some(database) = db() else { return String::new() };
    let Ok(read_txn) = database.begin_read() else { return String::new() };
    let Ok(table) = read_txn.open_table(META_TABLE) else { return String::new() };
    match table.get(META_KEY_GAME_PATH) {
        Ok(Some(guard)) => guard.value().to_string(),
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

// ─── Status ──────────────────────────────────────────────────────────────────

/// Response for `w3e/gamePath/status`.
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
