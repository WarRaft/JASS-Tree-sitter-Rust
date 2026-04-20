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
    let snapshot = _build_snapshot_inner(archive_path);

    // Pre-serialise to JSON once.
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

/// Build a snapshot with w3d/w3b merges from the given archive.
///
/// Returns pre-serialised JSON bytes.  Unlike [`build_snapshot`], this does NOT
/// cache the result — each map archive may have different w3d/w3b data.
pub fn build_snapshot_for_archive(archive_path: &str) -> Vec<u8> {
    let mut snapshot = _build_snapshot_inner(Some(archive_path));

    // Merge war3map.w3d into doodads.
    if let Some(ref mut dood_result) = snapshot.doodads_slk {
        if let Ok(archive) = storm_rs::MpqArchive::open(archive_path) {
            // Load WTS for TRIGSTR_ resolution in w3d names.
            if let Ok(wts_buf) = archive.read_file("war3map.wts") {
                super::westrings::load_map_strings(&wts_buf, "war3map.wts");
            }
            if let Ok(w3d_buf) = archive.read_file("war3map.w3d") {
                match crate::lng::w3abdhqtu::parse::W3ObjectData::read(&w3d_buf, true) {
                    Ok((w3d_data, _meta)) => {
                        dood_result.doodads_default = dood_result.doodads.clone();
                        let dood_meta = super::slk::load_doodad_metadata();
                        let errs = super::slk::merge_w3d_into_doodads(
                            &mut dood_result.doodads,
                            &w3d_data,
                            &dood_meta,
                        );
                        dood_result.w3d_errors = errs;
                    }
                    Err(e) => {
                        dood_result.w3d_errors.push(format!("Failed to parse war3map.w3d: {}", e));
                    }
                }
            }
        }
    }

    // Merge war3map.w3b into destructables.
    if let Some(ref mut dest_result) = snapshot.destructables_slk {
        if let Ok(archive) = storm_rs::MpqArchive::open(archive_path) {
            if let Ok(w3b_buf) = archive.read_file("war3map.w3b") {
                match crate::lng::w3abdhqtu::parse::W3ObjectData::read(&w3b_buf, false) {
                    Ok((w3b_data, _meta)) => {
                        dest_result.destructables_default = dest_result.destructables.clone();
                        let dest_meta = super::slk::load_destructable_metadata();
                        let errs = super::slk::merge_w3b_into_destructables(
                            &mut dest_result.destructables,
                            &w3b_data,
                            &dest_meta,
                        );
                        dest_result.w3b_errors = errs;
                    }
                    Err(e) => {
                        dest_result.w3b_errors.push(format!("Failed to parse war3map.w3b: {}", e));
                    }
                }
            }
        }
    }

    log::info!(
        "Archive snapshot built for '{}': doodads={}, destructables={}",
        archive_path,
        snapshot.doodads_slk.is_some(),
        snapshot.destructables_slk.is_some(),
    );

    serde_json::to_vec(&snapshot).unwrap_or_default()
}

fn _build_snapshot_inner(archive_path: Option<&str>) -> GameSnapshot {
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

    GameSnapshot {
        westrings,
        terrain_slk,
        doodads_slk,
        units_slk,
        destructables_slk,
        cliff_types_slk,
        cliff_variations,
        water_slk,
    }
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

// ─── Decorations payload (archive-specific) ──────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecorationPlaced {
    pub raw: u32,
    pub text: String,
    pub variation: u32,
    pub position: crate::lng::doo::parse::Vector,
    pub angle: f32,
    pub scale: crate::lng::doo::parse::Vector,
    pub kind: String,
    pub model_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecorationsPayload {
    pub doodads_raw: Option<DoodadsSlkResult>,
    pub doodads_merged: Option<DoodadsSlkResult>,
    pub destructables_raw: Option<DestructablesSlkResult>,
    pub destructables_merged: Option<DestructablesSlkResult>,
    pub placed: Vec<DecorationPlaced>,
}

fn _resolve_model_set_cached(
    file: &str,
    archive_path: &str,
    tileset: Option<&str>,
    cache: &mut HashMap<String, Vec<String>>,
) -> Vec<String> {
    if file.is_empty() {
        return Vec::new();
    }
    let key = file.to_ascii_lowercase();
    if let Some(v) = cache.get(&key) {
        return v.clone();
    }
    let set: Vec<String> = super::file_lookup::resolve_model_variants_ext(file, Some(archive_path), tileset)
        .into_iter()
        .map(|v| v.path)
        .collect();
    cache.insert(key, set.clone());
    set
}

/// Build decorations-only payload for a map archive:
/// - raw SLK copies
/// - merged copies with w3d/w3b applied
/// - placed doodad/destructable entries from war3map.doo
pub fn build_decorations_for_archive(archive_path: &str) -> Vec<u8> {
    use super::slk::{
        load_destructable_metadata, load_destructables_slk, load_doodad_metadata, load_doodads_slk,
        merge_w3b_into_destructables, merge_w3d_into_doodads,
    };

    super::game_path::discover_tileset_mpqs();
    super::westrings::ensure_loaded(Some(archive_path));

    // Read tileset from war3map.w3e for tileset-specific lookup paths.
    let tileset = storm_rs::MpqArchive::open(archive_path)
        .ok()
        .and_then(|a| a.read_file("war3map.w3e").ok())
        .and_then(|buf| crate::lng::w3e::parse::W3eData::read(&buf).ok().map(|(d, _)| d.tileset));

    let mut doodads_merged = load_doodads_slk(Some(archive_path));
    let mut destructables_merged = load_destructables_slk(Some(archive_path));
    let mut doodads_raw = doodads_merged.clone();
    let mut destructables_raw = destructables_merged.clone();

    if let Ok(archive) = storm_rs::MpqArchive::open(archive_path) {
        if let Ok(wts_buf) = archive.read_file("war3map.wts") {
            super::westrings::load_map_strings(&wts_buf, "war3map.wts");
        }

        if let (Some(dood), Ok(w3d_buf)) = (&mut doodads_merged, archive.read_file("war3map.w3d")) {
            match crate::lng::w3abdhqtu::parse::W3ObjectData::read(&w3d_buf, true) {
                Ok((w3d_data, _)) => {
                    dood.doodads_default = dood.doodads.clone();
                    let meta = load_doodad_metadata();
                    dood.w3d_errors = merge_w3d_into_doodads(&mut dood.doodads, &w3d_data, &meta);
                }
                Err(e) => dood.w3d_errors.push(format!("Failed to parse war3map.w3d: {e}")),
            }
        }

        if let (Some(dest), Ok(w3b_buf)) = (&mut destructables_merged, archive.read_file("war3map.w3b")) {
            match crate::lng::w3abdhqtu::parse::W3ObjectData::read(&w3b_buf, false) {
                Ok((w3b_data, _)) => {
                    dest.destructables_default = dest.destructables.clone();
                    let meta = load_destructable_metadata();
                    dest.w3b_errors = merge_w3b_into_destructables(&mut dest.destructables, &w3b_data, &meta);
                }
                Err(e) => dest.w3b_errors.push(format!("Failed to parse war3map.w3b: {e}")),
            }
        }
    }

    let mut placed: Vec<DecorationPlaced> = Vec::new();
    let mut model_cache: HashMap<String, Vec<String>> = HashMap::new();

    if let Ok(archive) = storm_rs::MpqArchive::open(archive_path) {
        if let Ok(doo_buf) = archive.read_file("war3map.doo") {
            if let Ok((doo, _)) = crate::lng::doo::parse::DooData::read(&doo_buf, false, 26) {
                for it in doo.items {
                    let raw = it.rawcode.raw;
                    let mut kind = String::from("unknown");
                    let mut selected = String::new();

                    if let Some(ref mut dood) = doodads_merged {
                        if let Some(d) = dood.doodads.get_mut(&raw) {
                            kind = String::from("doodad");
                            if d.model_set.is_empty() {
                                d.model_set = _resolve_model_set_cached(&d.file, archive_path, tileset.as_deref(), &mut model_cache);
                            }
                            if !d.model_set.is_empty() {
                                selected = d.model_set[(it.variation as usize) % d.model_set.len()].clone();
                            }
                            if let Some(ref mut raw_set) = doodads_raw {
                                if let Some(rd) = raw_set.doodads.get_mut(&raw) {
                                    if rd.model_set.is_empty() {
                                        rd.model_set = d.model_set.clone();
                                    }
                                }
                            }
                        }
                    }

                    if kind == "unknown" {
                        if let Some(ref mut dest) = destructables_merged {
                            if let Some(d) = dest.destructables.get_mut(&raw) {
                                kind = String::from("destructable");
                                if d.model_set.is_empty() {
                                    d.model_set = _resolve_model_set_cached(&d.file, archive_path, tileset.as_deref(), &mut model_cache);
                                }
                                if !d.model_set.is_empty() {
                                    selected = d.model_set[(it.variation as usize) % d.model_set.len()].clone();
                                }
                                if let Some(ref mut raw_set) = destructables_raw {
                                    if let Some(rd) = raw_set.destructables.get_mut(&raw) {
                                        if rd.model_set.is_empty() {
                                            rd.model_set = d.model_set.clone();
                                        }
                                    }
                                }
                            }
                        }
                    }

                    placed.push(DecorationPlaced {
                        raw,
                        text: it.rawcode.text,
                        variation: it.variation,
                        position: it.position,
                        angle: it.angle,
                        scale: it.scale,
                        kind,
                        model_path: selected,
                    });
                }
            }
        }
    }

    let payload = DecorationsPayload {
        doodads_raw,
        doodads_merged,
        destructables_raw,
        destructables_merged,
        placed,
    };

    serde_json::to_vec(&payload).unwrap_or_default()
}

