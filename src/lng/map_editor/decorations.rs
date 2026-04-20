//! Decorations payload — doodads + destructables placed on the map.
//!
//! Built per-archive from SLK catalogs, w3d/w3b overrides, and war3map.doo.

use serde::Serialize;
use std::collections::HashMap;
use std::time::Instant;

use super::slk::{DoodadsSlkResult, DestructablesSlkResult};

// ─── Structs ─────────────────────────────────────────────────────────────────

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
    /// Skin override rawcode (patch ≥ 32).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin: Option<u32>,
    /// Visibility / collision flag.
    pub flag: crate::lng::doo::parse::Flag,
    /// Doodad health percentage (0–100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<u8>,
    /// Doodad editor ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn _resolve_model_set_cached(
    file: &str,
    lookuper: &super::file_lookup::Lookuper,
    cache: &mut HashMap<String, Vec<String>>,
) -> Vec<String> {
    if file.is_empty() {
        return Vec::new();
    }
    let key = file.to_ascii_lowercase();
    if let Some(v) = cache.get(&key) {
        return v.clone();
    }
    let set: Vec<String> = lookuper
        .resolve_model_variants(file)
        .into_iter()
        .map(|v| v.path)
        .collect();
    cache.insert(key, set.clone());
    set
}

// ─── Builder ─────────────────────────────────────────────────────────────────

/// Build decorations-only payload for a map archive:
/// - raw SLK copies
/// - merged copies with w3d/w3b applied
/// - placed doodad/destructable entries from war3map.doo + validation errors
pub fn build_decorations_for_archive(archive_path: &str) -> DecorationsPayload {
    use super::slk::{
        load_destructable_metadata, load_destructables_slk, load_doodad_metadata, load_doodads_slk,
        merge_w3b_into_destructables, merge_w3d_into_doodads,
    };

    let total_started_at = Instant::now();
    crate::debug_log!("map_editor::decorations START archive={}", archive_path);

    super::game_path::discover_tileset_mpqs();
    super::westrings::ensure_loaded(Some(archive_path));

    // Read tileset from war3map.w3e for tileset-specific lookup paths.
    let pre_archive = storm_rs::MpqArchive::open(archive_path).ok();
    let tileset = pre_archive
        .as_ref()
        .and_then(|a| a.read_file("war3map.w3e").ok())
        .and_then(|buf| {
            crate::lng::w3e::parse::W3eData::read(&buf)
                .ok()
                .map(|(d, _)| d.tileset)
        });

    // Build the Lookuper once — all archive handles are opened here and reused
    // for every model-variant resolution below.
    let game_path = super::game_path::get_game_path();
    let lookuper = super::file_lookup::Lookuper::new(
        Some(archive_path),
        tileset.as_deref(),
        if game_path.is_empty() { None } else { Some(game_path.as_str()) },
    );

    let mut doodads_merged = load_doodads_slk(Some(archive_path));
    let mut destructables_merged = load_destructables_slk(Some(archive_path));

    let mut doodads_raw = doodads_merged.clone();
    let mut destructables_raw = destructables_merged.clone();

    if let Some(wts_buf) = lookuper.read_map_file("war3map.wts") {
        super::westrings::load_map_strings(&wts_buf, "war3map.wts");
    }

    if let (Some(dood), Some(w3d_buf)) = (&mut doodads_merged, lookuper.read_map_file("war3map.w3d")) {
        match crate::lng::w3abdhqtu::parse::W3ObjectData::read(&w3d_buf, true) {
            Ok((w3d_data, _)) => {
                dood.doodads_default = dood.doodads.clone();
                let meta = load_doodad_metadata();
                dood.w3d_errors = merge_w3d_into_doodads(&mut dood.doodads, &w3d_data, &meta);
            }
            Err(e) => {
                dood.w3d_errors.push(format!("Failed to parse war3map.w3d: {e}"));
            }
        }
    }

    if let (Some(dest), Some(w3b_buf)) = (&mut destructables_merged, lookuper.read_map_file("war3map.w3b")) {
        match crate::lng::w3abdhqtu::parse::W3ObjectData::read(&w3b_buf, false) {
            Ok((w3b_data, _)) => {
                dest.destructables_default = dest.destructables.clone();
                let meta = load_destructable_metadata();
                dest.w3b_errors = merge_w3b_into_destructables(&mut dest.destructables, &w3b_data, &meta);
            }
            Err(e) => {
                dest.w3b_errors.push(format!("Failed to parse war3map.w3b: {e}"));
            }
        }
    }

    let mut placed: Vec<DecorationPlaced> = Vec::new();
    let mut model_cache: HashMap<String, Vec<String>> = HashMap::new();

    if let Some(doo_buf) = lookuper.read_map_file("war3map.doo") {
        if let Ok((doo, _)) = crate::lng::doo::parse::DooData::read(&doo_buf, false, 26) {
            for it in doo.items {
                let raw = it.rawcode.raw;
                let mut kind = String::from("unknown");
                let mut selected = String::new();
                let mut error: Option<String> = None;

                let in_doodads = doodads_merged
                    .as_ref()
                    .map(|m| m.doodads.contains_key(&raw))
                    .unwrap_or(false);
                let in_dest = destructables_merged
                    .as_ref()
                    .map(|m| m.destructables.contains_key(&raw))
                    .unwrap_or(false);

                if in_doodads && in_dest {
                    error = Some(String::from("Rawcode exists in both doodads and destructables"));
                }

                if let Some(ref mut dood) = doodads_merged {
                    if let Some(d) = dood.doodads.get_mut(&raw) {
                        kind = String::from("doodad");
                        if d.model_set.is_empty() {
                            d.model_set = _resolve_model_set_cached(&d.file, &lookuper, &mut model_cache);
                        }
                        if !d.model_set.is_empty() {
                            selected = d.model_set[(it.variation as usize) % d.model_set.len()].clone();
                        } else {
                            error = Some(String::from("No valid model variants for doodad"));
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
                                d.model_set = _resolve_model_set_cached(&d.file, &lookuper, &mut model_cache);
                            }
                            if !d.model_set.is_empty() {
                                selected = d.model_set[(it.variation as usize) % d.model_set.len()].clone();
                            } else {
                                error = Some(String::from("No valid model variants for destructable"));
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

                if kind == "unknown" {
                    error = Some(String::from("Rawcode not found in doodads/destructables catalogs"));
                }

                let health = it.doodad.as_ref().map(|d| d.health);
                let num = it.doodad.as_ref().map(|d| d.num);

                placed.push(DecorationPlaced {
                    raw,
                    text: it.rawcode.text,
                    variation: it.variation,
                    position: it.position,
                    angle: it.angle,
                    scale: it.scale,
                    kind,
                    model_path: selected,
                    skin: it.skin,
                    flag: it.flag,
                    health,
                    num,
                    error,
                });
            }
        }
    }

    crate::debug_log!(
        "map_editor::decorations END archive={}, elapsed_ms={}, placed={}",
        archive_path,
        total_started_at.elapsed().as_millis(),
        placed.len(),
    );

    DecorationsPayload {
        doodads_raw,
        doodads_merged,
        destructables_raw,
        destructables_merged,
        placed,
    }
}

pub fn serialize_decorations_json(payload: &DecorationsPayload) -> Vec<u8> {
    serde_json::to_vec(payload).unwrap_or_default()
}
