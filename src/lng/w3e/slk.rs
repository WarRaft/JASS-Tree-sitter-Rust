//! Generic SYLK (`.slk`) parser and terrain tile metadata loader.
//!
//! The parser reads raw SLK bytes and returns rows as `Vec<HashMap<String, String>>`.
//! Then `load_terrain_slk` uses the cascading file lookup to find
//! `TerrainArt\Terrain.slk` and extracts the columns we need.

use serde::Serialize;
use std::collections::HashMap;

// ─── Generic SLK parser ──────────────────────────────────────────────────────

/// Parse a SYLK file into a list of row maps.
///
/// Row 1 is treated as headers; every subsequent row becomes a
/// `HashMap<header, value>`.  Returns an empty vec on malformed input.
pub fn parse_slk(data: &[u8]) -> Vec<HashMap<String, String>> {
    let text = String::from_utf8_lossy(data);

    let mut cols: usize = 0;
    let mut rows: usize = 0;
    let mut headers: Vec<String> = Vec::new();
    let mut result: Vec<HashMap<String, String>> = Vec::new();

    // Sticky coordinates (SYLK carries forward the last X / Y seen).
    let mut cur_x: usize = 1;
    let mut cur_y: usize = 1;

    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }

        // ── B record: dimensions ─────────────────────────────────
        if line.starts_with("B;") {
            for part in line[2..].split(';') {
                if let Some(v) = part.strip_prefix('X') {
                    cols = v.parse().unwrap_or(0);
                } else if let Some(v) = part.strip_prefix('Y') {
                    rows = v.parse().unwrap_or(0);
                }
            }
            if rows > 1 {
                result.reserve(rows - 1);
            }
            if cols > 0 {
                headers.resize(cols, String::new());
            }
            continue;
        }

        // ── C record: cell value ─────────────────────────────────
        if line.starts_with("C;") {
            let mut x: Option<usize> = None;
            let mut y: Option<usize> = None;
            let mut k_value: Option<&str> = None;

            for part in line[2..].split(';') {
                if let Some(v) = part.strip_prefix('X') {
                    x = v.parse().ok();
                } else if let Some(v) = part.strip_prefix('Y') {
                    y = v.parse().ok();
                } else if let Some(v) = part.strip_prefix('K') {
                    k_value = Some(v);
                }
            }

            if let Some(yy) = y {
                cur_y = yy;
            }
            if let Some(xx) = x {
                cur_x = xx;
            }

            let Some(raw_k) = k_value else { continue };

            // Strip surrounding quotes from string values.
            let value = if raw_k.starts_with('"') && raw_k.ends_with('"') && raw_k.len() >= 2 {
                &raw_k[1..raw_k.len() - 1]
            } else {
                raw_k
            };

            let ci = cur_x.saturating_sub(1); // 0-based column index

            if cur_y == 1 {
                // Header row
                if ci < headers.len() {
                    headers[ci] = value.to_string();
                } else {
                    // SLK without a B record, or extra columns
                    if headers.len() <= ci {
                        headers.resize(ci + 1, String::new());
                    }
                    headers[ci] = value.to_string();
                }
            } else {
                // Data row (1-based row 2 → result index 0)
                let ri = cur_y - 2;
                if result.len() <= ri {
                    result.resize_with(ri + 1, HashMap::new);
                }
                if ci < headers.len() && !headers[ci].is_empty() {
                    result[ri].insert(headers[ci].clone(), value.to_string());
                }
            }

            continue;
        }

        // All other records (ID, F, E, …) are ignored.
    }

    result
}

// ─── Terrain tile info ───────────────────────────────────────────────────────

/// A single terrain tile entry extracted from `Terrain.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerrainTileInfo {
    pub tile_id: String,
    pub dir: String,
    pub file: String,
    pub comment: String,
    /// Resolved texture extension: `".tga"`, `".blp"`, or `""` if not found.
    pub ext: String,
}

/// Result of loading `TerrainArt\Terrain.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerrainSlkResult {
    /// Where the file was found (e.g. `"War3Patch.mpq"`, `"game folder"`, …).
    pub source: String,
    /// All tile rows from the SLK.
    pub tiles: Vec<TerrainTileInfo>,
}

/// Try to load and parse `TerrainArt\Terrain.slk` via the cascading lookup.
pub fn load_terrain_slk(archive_path: Option<&str>) -> Option<TerrainSlkResult> {
    let (buf, source) = super::file_lookup::lookup_file(
        "TerrainArt\\Terrain.slk",
        archive_path,
    )?;

    let rows = parse_slk(&buf);

    let tiles: Vec<TerrainTileInfo> = rows
        .into_iter()
        .filter_map(|row| {
            let tile_id = row.get("tileID")?.clone();
            if tile_id.is_empty() {
                return None;
            }
            let dir = row.get("dir").cloned().unwrap_or_default();
            let file = row.get("file").cloned().unwrap_or_default();

            // Resolve texture extension: .tga first, then .blp
            let ext = if !dir.is_empty() && !file.is_empty() {
                let base = format!("{}\\{}", dir, file);
                if super::file_lookup::lookup_file_exists(
                    &format!("{}.tga", base),
                    archive_path,
                ) {
                    ".tga".to_string()
                } else if super::file_lookup::lookup_file_exists(
                    &format!("{}.blp", base),
                    archive_path,
                ) {
                    ".blp".to_string()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            Some(TerrainTileInfo {
                tile_id,
                dir,
                file,
                comment: row.get("comment").cloned().unwrap_or_default(),
                ext,
            })
        })
        .collect();

    Some(TerrainSlkResult {
        source: source.to_string(),
        tiles,
    })
}

// ─── Doodad info ─────────────────────────────────────────────────────────────

/// A single doodad entry extracted from `Doodads\Doodads.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoodadInfo {
    pub dood_id: String,
    pub category: String,
    pub tilesets: String,
    pub file: String,
    pub comment: String,
    pub name: String,
    pub dood_class: String,
    pub num_var: u32,
    pub def_scale: f64,
    pub min_scale: f64,
    pub max_scale: f64,
}

/// Result of loading `Doodads\Doodads.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoodadsSlkResult {
    /// Where the file was found (e.g. `"War3Patch.mpq"`, `"game folder"`, …).
    pub source: String,
    /// All doodad rows from the SLK.
    pub doodads: Vec<DoodadInfo>,
}

/// Try to load and parse `Doodads\Doodads.slk` via the cascading lookup.
pub fn load_doodads_slk(archive_path: Option<&str>) -> Option<DoodadsSlkResult> {
    let (buf, source) = super::file_lookup::lookup_file(
        "Doodads\\Doodads.slk",
        archive_path,
    )?;

    let rows = parse_slk(&buf);

    let doodads: Vec<DoodadInfo> = rows
        .into_iter()
        .filter_map(|row| {
            let dood_id = row.get("doodID")?.clone();
            if dood_id.is_empty() {
                return None;
            }
            Some(DoodadInfo {
                dood_id,
                category: row.get("category").cloned().unwrap_or_default(),
                tilesets: row.get("tilesets").cloned().unwrap_or_default(),
                file: row.get("file").cloned().unwrap_or_default(),
                comment: row.get("comment").cloned().unwrap_or_default(),
                name: row.get("Name").cloned().unwrap_or_default(),
                dood_class: row.get("doodClass").cloned().unwrap_or_default(),
                num_var: row.get("numVar").and_then(|v| v.parse().ok()).unwrap_or(0),
                def_scale: row.get("defScale").and_then(|v| v.parse().ok()).unwrap_or(1.0),
                min_scale: row.get("minScale").and_then(|v| v.parse().ok()).unwrap_or(0.0),
                max_scale: row.get("maxScale").and_then(|v| v.parse().ok()).unwrap_or(0.0),
            })
        })
        .collect();

    Some(DoodadsSlkResult {
        source: source.to_string(),
        doodads,
    })
}

// ─── Unit info ───────────────────────────────────────────────────────────────

/// A single unit entry extracted from `Units\UnitData.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitInfo {
    pub unit_id: String,
    pub sort: String,
    pub comment: String,
    pub race: String,
    pub move_tp: String,
    pub threat: u32,
    pub points: u32,
    pub death: f64,
    pub can_sleep: bool,
    pub cargo_size: u32,
    pub can_flee: bool,
}

/// Result of loading `Units\UnitData.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitsSlkResult {
    /// Where the file was found (e.g. `"War3Patch.mpq"`, `"game folder"`, …).
    pub source: String,
    /// All unit rows from the SLK.
    pub units: Vec<UnitInfo>,
}

/// Try to load and parse `Units\UnitData.slk` via the cascading lookup.
pub fn load_units_slk(archive_path: Option<&str>) -> Option<UnitsSlkResult> {
    let (buf, source) = super::file_lookup::lookup_file(
        "Units\\UnitData.slk",
        archive_path,
    )?;

    let rows = parse_slk(&buf);

    let units: Vec<UnitInfo> = rows
        .into_iter()
        .filter_map(|row| {
            let unit_id = row.get("unitID")?.clone();
            if unit_id.is_empty() {
                return None;
            }
            Some(UnitInfo {
                unit_id,
                sort: row.get("sort").cloned().unwrap_or_default(),
                comment: row.get("comment(s)").cloned().unwrap_or_default(),
                race: row.get("race").cloned().unwrap_or_default(),
                move_tp: row.get("movetp").cloned().unwrap_or_default(),
                threat: row.get("threat").and_then(|v| v.parse().ok()).unwrap_or(0),
                points: row.get("points").and_then(|v| v.parse().ok()).unwrap_or(0),
                death: row.get("death").and_then(|v| v.parse().ok()).unwrap_or(0.0),
                can_sleep: row.get("canSleep").map(|v| v == "1").unwrap_or(false),
                cargo_size: row.get("cargoSize").and_then(|v| v.parse().ok()).unwrap_or(0),
                can_flee: row.get("canFlee").map(|v| v == "1").unwrap_or(false),
            })
        })
        .collect();

    Some(UnitsSlkResult {
        source: source.to_string(),
        units,
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_terrain_slk_fixture() {
        let data = include_bytes!("../../lng/slk/fixtures/TerrainArt/Terrain.slk");
        let rows = parse_slk(data);
        // The fixture has 177 data rows (Y2..Y178)
        assert!(!rows.is_empty(), "should parse some rows");

        let first = &rows[0];
        assert_eq!(first.get("tileID").map(|s| s.as_str()), Some("Ldrt"));
        assert_eq!(first.get("comment").map(|s| s.as_str()), Some("Dirt"));
        assert_eq!(
            first.get("dir").map(|s| s.as_str()),
            Some("TerrainArt\\LordaeronSummer")
        );
        assert_eq!(
            first.get("file").map(|s| s.as_str()),
            Some("Lords_Dirt")
        );
    }

    #[test]
    fn parse_doodads_slk_fixture() {
        let data = include_bytes!("../../lng/slk/fixtures/Doodads/Doodads.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some rows");

        let first = &rows[0];
        assert_eq!(first.get("doodID").map(|s| s.as_str()), Some("APms"));
        assert_eq!(first.get("comment").map(|s| s.as_str()), Some("Mushrooms"));
        assert!(first.get("file").is_some());
    }

    #[test]
    fn parse_units_slk_fixture() {
        let data = include_bytes!("../../lng/slk/fixtures/Units/UnitData.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some rows");

        let first = &rows[0];
        assert_eq!(first.get("unitID").map(|s| s.as_str()), Some("Hamg"));
        assert_eq!(first.get("comment(s)").map(|s| s.as_str()), Some("HeroArchMage"));
        assert_eq!(first.get("race").map(|s| s.as_str()), Some("human"));
    }
}

