//! Game data snapshot — all SLK / INI data loaded once and cached.
//!
//! When the game path is set, [`build_snapshot`] eagerly reads every file we
//! need (terrain, doodads, units, destructables, westrings) and packs them
//! into a single [`GameSnapshot`].  The snapshot is then serialised to JSON
//! once and cached as a `Vec<u8>`.
//!
//! The HTTP endpoint `/w3e/snapshot` returns this pre-built blob directly —
//! zero per-request work.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;

use super::slk::{
    DoodadsSlkResult, DestructablesSlkResult, TerrainSlkResult, UnitsSlkResult,
    CliffTypesSlkResult, CliffVariationsResult, WaterSlkResult,
};

// ─── Snapshot struct ─────────────────────────────────────────────────────────

/// Top-level snapshot: everything the client needs in one response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSnapshot {
    /// WESTRING_* resolution map (key → resolved value).
    pub westrings: HashMap<String, String>,
    /// Terrain tile data from `TerrainArt\Terrain.slk`.
    pub terrain_slk: Option<TerrainSlkResult>,
    /// Doodad catalog from `Doodads\Doodads.slk`.
    pub doodads_slk: Option<DoodadsSlkResult>,
    /// Unit catalog from merged Unit*.slk + *UnitStrings.txt.
    pub units_slk: Option<UnitsSlkResult>,
    /// Destructable catalog from `Units\DestructableData.slk`.
    pub destructables_slk: Option<DestructablesSlkResult>,
    /// Cliff type catalog from `TerrainArt\CliffTypes.slk`.
    pub cliff_types_slk: Option<CliffTypesSlkResult>,
    /// Max variation per cliff letter-pattern (from embedded Cliffs.slk / CityCliffs.slk).
    pub cliff_variations: Option<CliffVariationsResult>,
    /// Water parameters from `TerrainArt\Water.slk` for the current tileset.
    pub water_slk: Option<WaterSlkResult>,
}

// ─── Global cache ────────────────────────────────────────────────────────────

/// Cached pre-serialised JSON bytes + the typed snapshot.
struct CachedSnapshot {
    json: Vec<u8>,
    #[allow(dead_code)]
    data: GameSnapshot,
}

static SNAPSHOT: Mutex<Option<CachedSnapshot>> = Mutex::new(None);

/// Build (or re-build) the snapshot from the game installation.
///
/// Called when the game path is set or changed.  Reads all SLK / INI files
/// synchronously (should be called from `spawn_blocking`).
pub fn build_snapshot(archive_path: Option<&str>) {
    use super::slk::{load_terrain_slk, load_doodads_slk, load_units_slk, load_destructables_slk, load_cliff_types_slk, load_cliff_variations, load_water_slk};

    // 0. Discover tileset MPQs (loose files + inside War3*.mpq archives).
    super::game_path::discover_tileset_mpqs();

    // 1. Ensure westrings are loaded first (all SLK loaders depend on them).
    super::westrings::ensure_loaded(archive_path);
    let westrings = super::westrings::get_all();

    // 2. Load all SLK data.
    let terrain_slk = load_terrain_slk(archive_path);
    let doodads_slk = load_doodads_slk(archive_path);
    let units_slk = load_units_slk(archive_path);
    let destructables_slk = load_destructables_slk(archive_path);
    let cliff_types_slk = load_cliff_types_slk(archive_path, None);
    let cliff_variations = Some(load_cliff_variations());

    // Water SLK needs the tileset letter (set when w3e is parsed).
    let water_slk = super::game_path::get_tileset()
        .and_then(|ts| load_water_slk(archive_path, &ts));

    let snapshot = GameSnapshot {
        westrings,
        terrain_slk,
        doodads_slk,
        units_slk,
        destructables_slk,
        cliff_types_slk,
        cliff_variations,
        water_slk,
    };

    // 3. Pre-serialise to JSON once.
    let json = serde_json::to_vec(&snapshot).unwrap_or_default();

    log::info!(
        "Game snapshot built: {} bytes ({} westrings, terrain={} (source: {}), doodads={}, units={}, destructables={}, cliffTypes={} (source: {}), cliffVariations={})",
        json.len(),
        snapshot.westrings.len(),
        snapshot.terrain_slk.is_some(),
        snapshot.terrain_slk.as_ref().map(|s| s.source.as_str()).unwrap_or("N/A"),
        snapshot.doodads_slk.is_some(),
        snapshot.units_slk.is_some(),
        snapshot.destructables_slk.is_some(),
        snapshot.cliff_types_slk.is_some(),
        snapshot.cliff_types_slk.as_ref().map(|s| s.source.as_str()).unwrap_or("N/A"),
        snapshot.cliff_variations.is_some(),
    );

    let mut guard = match SNAPSHOT.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    *guard = Some(CachedSnapshot { json, data: snapshot });
}

/// Return the cached JSON bytes (or `None` if never built).
pub fn get_snapshot_json() -> Option<Vec<u8>> {
    let guard = match SNAPSHOT.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    guard.as_ref().map(|c| c.json.clone())
}

/// Drop the cached snapshot (called when game path changes before re-build).
pub fn invalidate() {
    let mut guard = match SNAPSHOT.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    *guard = None;
}

