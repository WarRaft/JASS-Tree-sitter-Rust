//! Units payload — units placed on the map.
//!
//! Built per-archive from SLK catalogs, w3u overrides, and war3mapUnits.doo.

use serde::Serialize;

use super::slk::UnitsSlkResult;

// ─── Structs ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitPlaced {
    pub raw: u32,
    pub text: String,
    pub variation: u32,
    pub position: crate::lng::doo::parse::Vector,
    pub angle: f32,
    pub scale: crate::lng::doo::parse::Vector,
    pub player: u32,
    pub model_path: String,
    /// Skin override rawcode (patch ≥ 32).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin: Option<u32>,
    /// Visibility / collision flag.
    pub flag: crate::lng::doo::parse::Flag,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitsPayload {
    pub units_raw: Option<UnitsSlkResult>,
    pub units_merged: Option<UnitsSlkResult>,
    pub placed: Vec<UnitPlaced>,
}

// ─── Builder ─────────────────────────────────────────────────────────────────

/// Build units-only payload for a map archive:
/// - raw SLK copies
/// - merged copies with w3u applied
/// - placed unit entries from war3mapUnits.doo
pub fn build_units_for_archive(archive_path: &str) -> UnitsPayload {
    use super::slk::{load_unit_metadata, load_units_slk, merge_w3u_into_units};

    super::game_path::discover_tileset_mpqs();
    super::westrings::ensure_loaded(Some(archive_path));

    let mut units_merged = load_units_slk(Some(archive_path));
    let units_raw = units_merged.clone();

    if let Ok(archive) = storm_rs::MpqArchive::open(archive_path) {
        if let Ok(wts_buf) = archive.read_file("war3map.wts") {
            super::westrings::load_map_strings(&wts_buf, "war3map.wts");
        }

        if let (Some(unit_result), Ok(w3u_buf)) = (&mut units_merged, archive.read_file("war3map.w3u")) {
            match crate::lng::w3abdhqtu::parse::W3ObjectData::read(&w3u_buf, false) {
                Ok((w3u_data, _)) => {
                    unit_result.units_default = unit_result.units.clone();
                    let meta = load_unit_metadata();
                    unit_result.w3u_errors = merge_w3u_into_units(&mut unit_result.units, &w3u_data, &meta);
                }
                Err(e) => unit_result.w3u_errors.push(format!("Failed to parse war3map.w3u: {e}")),
            }
        }
    }

    let mut placed: Vec<UnitPlaced> = Vec::new();

    if let Ok(archive) = storm_rs::MpqArchive::open(archive_path) {
        if let Ok(doo_buf) = archive.read_file("war3mapUnits.doo") {
            if let Ok((doo, _)) = crate::lng::doo::parse::DooData::read(&doo_buf, true, 26) {
                for it in doo.items {
                    let raw = it.rawcode.raw;
                    let mut model_path = String::new();
                    let mut error: Option<String> = None;

                    if let Some(ref unit_result) = units_merged {
                        if let Some(u) = unit_result.units.get(&raw) {
                            if !u.file.is_empty() {
                                model_path = u.file.clone();
                            }
                        } else {
                            error = Some(String::from("Rawcode not found in units catalog"));
                        }
                    }

                    let player = it.unit.as_ref().map(|u| u.player).unwrap_or(0);

                    placed.push(UnitPlaced {
                        raw,
                        text: it.rawcode.text,
                        variation: it.variation,
                        position: it.position,
                        angle: it.angle,
                        scale: it.scale,
                        player,
                        model_path,
                        skin: it.skin,
                        flag: it.flag,
                        error,
                    });
                }
            }
        }
    }

    UnitsPayload {
        units_raw,
        units_merged,
        placed,
    }
}

pub fn serialize_units_json(payload: &UnitsPayload) -> Vec<u8> {
    serde_json::to_vec(payload).unwrap_or_default()
}

