//! Shared embedded key-value store backed by [`redb`].
//!
//! All persistent caches (`file_cache`, `scope_resolver`, `import_graph`)
//! share a single `redb` database file.  This gives us:
//!
//! * **ACID transactions** — no more corrupted half-written blobs.
//! * **Memory-mapped reads** — structures are deserialized lazily, on demand.
//! * **Per-key writes** — only the changed entry is written, not the entire
//!   index.
//! * **Built-in version stamping** — when the extension version changes
//!   the `file_cache` and `scope` tables are purged automatically.
//!   The `import_graph` table is **kept** because it holds structural
//!   information (which files import which) that is independent of the
//!   serialisation format.  Files are rescanned lazily — each tree is
//!   re-parsed from disk the first time the user opens a file from it.
//!
//! Database path: `$CACHE_DIR/jass-tree-sitter.redb`

use log::{error, info, warn};
use once_cell::sync::Lazy;
use redb::{Database, ReadableDatabase, TableDefinition};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

// ─── Version ─────────────────────────────────────────────────────────────────

/// Extension version extracted from `package.json` at **compile time**.
/// Any change triggers an automatic cache purge on next server start.
pub const EXT_VERSION: &str = env!("EXT_VERSION");

/// Cache schema version — bump when the on-disk format changes
/// (new fields in `CacheEntry`, `GlobalEntry`, etc.) independently
/// of the extension version.
pub const SCHEMA_VERSION: u32 = 2;

/// Combined version key stored in the database.
fn version_key() -> String {
    format!("{}-s{}", EXT_VERSION, SCHEMA_VERSION)
}

// ─── Database path ───────────────────────────────────────────────────────────

const DB_FILE: &str = "jass-tree-sitter.redb";

fn db_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join(DB_FILE))
}

// ─── Table definitions ───────────────────────────────────────────────────────

/// File cache: `URI string → bitcode(CacheEntry)`.
pub const FILE_CACHE_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("file_cache");

/// Scope resolver entries: `URI string → bitcode(ScopeFileData)`.
pub const SCOPE_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("scope");

/// Import graph edges: `URI string → bitcode(Vec<Url>)`.
pub const IMPORT_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("import_graph");

/// Metadata: `key → value`.
pub const META_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("meta");

const META_KEY_VERSION: &str = "version";

// ─── Global state ────────────────────────────────────────────────────────────

/// `true` if the database was purged on this startup (version mismatch).
static PURGED: AtomicBool = AtomicBool::new(false);

struct DbState {
    db: Database,
}

static DB_STATE: Lazy<Option<DbState>> = Lazy::new(|| {
    let path = db_path()?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let db = open_or_recreate(&path)?;

    // ── Version check ──
    let current = version_key();
    let stored = read_meta_version(&db);

    if stored.as_deref() != Some(current.as_str()) {
        info!(
            "cache_db: version mismatch (stored={:?}, current={:?}) — purging data caches",
            stored, current
        );
        purge_data_tables(&db);
        write_meta_version(&db, &current);
        PURGED.store(true, Ordering::SeqCst);
    } else {
        info!("cache_db: version OK ({})", current);
    }

    // ── Clean up legacy cache files ──
    cleanup_legacy_files(&path);

    Some(DbState { db })
});

// ─── Public API ──────────────────────────────────────────────────────────────

/// Get a reference to the shared database (if available).
pub fn db() -> Option<&'static Database> {
    DB_STATE.as_ref().map(|s| &s.db)
}



fn open_or_recreate(path: &PathBuf) -> Option<Database> {
    match Database::create(path) {
        Ok(db) => Some(db),
        Err(e) => {
            warn!("cache_db: open failed ({}) — recreating", e);
            let _ = std::fs::remove_file(path);
            match Database::create(path) {
                Ok(db) => Some(db),
                Err(e2) => {
                    error!("cache_db: recreate also failed: {}", e2);
                    None
                }
            }
        }
    }
}

fn read_meta_version(db: &Database) -> Option<String> {
    let read_txn = db.begin_read().ok()?;
    let table = read_txn.open_table(META_TABLE).ok()?;
    let guard = table.get(META_KEY_VERSION).ok()??;
    Some(guard.value().to_string())
}

fn write_meta_version(db: &Database, version: &str) {
    let write_txn = match db.begin_write() {
        Ok(t) => t,
        Err(e) => {
            error!("cache_db: begin_write (version): {}", e);
            return;
        }
    };
    {
        let mut table = match write_txn.open_table(META_TABLE) {
            Ok(t) => t,
            Err(e) => {
                error!("cache_db: open META_TABLE: {}", e);
                return;
            }
        };
        if let Err(e) = table.insert(META_KEY_VERSION, version) {
            error!("cache_db: insert version: {}", e);
            return;
        }
    }
    if let Err(e) = write_txn.commit() {
        error!("cache_db: commit version: {}", e);
    }
}

/// Purge format-dependent data tables (`file_cache`, `scope`).
///
/// The `import_graph` table is intentionally **kept** — it holds
/// structural "who imports whom" information that does not depend on
/// the serialisation format of parse results.  Keeping it means
/// `IMPORT_GRAPH.all_uris()` still returns the correct file set after
/// a version bump, so lazy per-tree rescanning works correctly.
fn purge_data_tables(db: &Database) {
    let write_txn = match db.begin_write() {
        Ok(t) => t,
        Err(e) => {
            error!("cache_db: begin_write (purge): {}", e);
            return;
        }
    };
    // Delete data tables — ignore errors for non-existent tables.
    let _ = write_txn.delete_table(FILE_CACHE_TABLE);
    let _ = write_txn.delete_table(SCOPE_TABLE);
    // NOTE: IMPORT_TABLE is kept intentionally.
    if let Err(e) = write_txn.commit() {
        error!("cache_db: commit purge: {}", e);
    }
    info!("cache_db: data tables purged (import_graph preserved)");
}

/// Remove legacy cache files from previous cache implementations.
fn cleanup_legacy_files(db_path: &PathBuf) {
    let cache_dir = match db_path.parent() {
        Some(d) => d,
        None => return,
    };

    // Old file_cache directory with individual .bin files.
    let old_dir = cache_dir.join("jass-tree-sitter-cache");
    if old_dir.is_dir() {
        if let Err(e) = std::fs::remove_dir_all(&old_dir) {
            warn!("cache_db: cleanup old cache dir: {}", e);
        } else {
            info!("cache_db: removed legacy cache dir {:?}", old_dir);
        }
    }

    // Old scope_resolver blob.
    let old_scope = cache_dir.join("jass-tree-sitter-scope.bin");
    if old_scope.is_file() {
        let _ = std::fs::remove_file(&old_scope);
        info!("cache_db: removed legacy scope cache");
    }

    // Old import_graph JSON.
    let old_graph = cache_dir.join("jass-tree-sitter-import-graph.json");
    if old_graph.is_file() {
        let _ = std::fs::remove_file(&old_graph);
        info!("cache_db: removed legacy import-graph cache");
    }
}

