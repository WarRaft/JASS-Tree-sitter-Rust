//! Water parameters from `TerrainArt\Water.slk`.

use serde::Serialize;
use std::collections::HashMap;
use super::parse_slk;

/// Per-tileset water parameters extracted from `TerrainArt\Water.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaterSlkEntry {
    /// Water height offset (e.g. `-0.7`).
    pub height: f64,
    /// Number of animated water textures (typically 45).
    pub num_tex: u32,
    /// Animation rate in ms per frame.
    pub tex_rate: u32,
    /// Texture file prefix (e.g. `"Water"` → `Water00.blp` .. `Water44.blp`).
    pub tex_file: String,

    /// Shallow min colour (RGBA 0–255).
    pub smin_r: u8, pub smin_g: u8, pub smin_b: u8, pub smin_a: u8,
    /// Shallow max colour (RGBA 0–255).
    pub smax_r: u8, pub smax_g: u8, pub smax_b: u8, pub smax_a: u8,
    /// Deep min colour (RGBA 0–255).
    pub dmin_r: u8, pub dmin_g: u8, pub dmin_b: u8, pub dmin_a: u8,
    /// Deep max colour (RGBA 0–255).
    pub dmax_r: u8, pub dmax_g: u8, pub dmax_b: u8, pub dmax_a: u8,
}

/// Result of loading `TerrainArt\Water.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaterSlkResult {
    pub source: String,
    pub entry: WaterSlkEntry,
}

/// Try to load and parse `TerrainArt\Water.slk` for the given tileset letter.
pub fn load_water_slk(archive_path: Option<&str>, tileset: &str) -> Option<WaterSlkResult> {
    let row_key = format!("{}Sha", tileset);
    log::info!("load_water_slk: looking for TerrainArt\\Water.slk, row={}", row_key);

    let (buf, source) = crate::lng::map_editor::file_lookup::lookup_file(
        "TerrainArt\\Water.slk",
        archive_path,
    ).unwrap_or_else(|| {
        // Fallback to embedded fixture
        let data = include_bytes!("../../../lng/slk/fixtures/TerrainArt/Water.slk");
        (data.to_vec(), "embedded fixture".to_string())
    });

    let rows = parse_slk(&buf);

    // Find the row whose first column (waterID) matches "{tileset}Sha"
    let row = rows.into_iter().find(|r| {
        r.get("waterID").map(|v| v == &row_key).unwrap_or(false)
    })?;

    fn col_u8(r: &HashMap<String, String>, k: &str) -> u8 {
        r.get(k).and_then(|v| v.parse().ok()).unwrap_or(0)
    }
    fn col_u32(r: &HashMap<String, String>, k: &str) -> u32 {
        r.get(k).and_then(|v| v.parse().ok()).unwrap_or(0)
    }
    fn col_f64(r: &HashMap<String, String>, k: &str) -> f64 {
        r.get(k).and_then(|v| v.parse().ok()).unwrap_or(0.0)
    }

    let entry = WaterSlkEntry {
        height: col_f64(&row, "height"),
        num_tex: col_u32(&row, "numTex"),
        tex_rate: col_u32(&row, "texRate"),
        tex_file: row.get("texFile").cloned().unwrap_or_default(),
        smin_r: col_u8(&row, "Smin_R"), smin_g: col_u8(&row, "Smin_G"),
        smin_b: col_u8(&row, "Smin_B"), smin_a: col_u8(&row, "Smin_A"),
        smax_r: col_u8(&row, "Smax_R"), smax_g: col_u8(&row, "Smax_G"),
        smax_b: col_u8(&row, "Smax_B"), smax_a: col_u8(&row, "Smax_A"),
        dmin_r: col_u8(&row, "Dmin_R"), dmin_g: col_u8(&row, "Dmin_G"),
        dmin_b: col_u8(&row, "Dmin_B"), dmin_a: col_u8(&row, "Dmin_A"),
        dmax_r: col_u8(&row, "Dmax_R"), dmax_g: col_u8(&row, "Dmax_G"),
        dmax_b: col_u8(&row, "Dmax_B"), dmax_a: col_u8(&row, "Dmax_A"),
    };

    log::info!("load_water_slk: found row '{}' in '{}' (height={}, numTex={}, texFile={})",
        row_key, source, entry.height, entry.num_tex, entry.tex_file);
    Some(WaterSlkResult {
        source: source.to_string(),
        entry,
    })
}

