//! Cliff type data from `TerrainArt\CliffTypes.slk` and cliff model variations.

use serde::Serialize;
use std::collections::HashMap;
use super::parse_slk;

/// A single cliff type entry extracted from `TerrainArt\CliffTypes.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliffTypeInfo {
    pub cliff_id: String,
    /// Directory name for cliff wall models (e.g. `"Cliffs"`, `"CityCliffs"`).
    pub cliff_model_dir: String,
    /// Directory name for ramp / slope transition models (e.g. `"CliffTrans"`).
    pub ramp_model_dir: String,
    /// Cliff class identifier (e.g. `"c1"`, `"c2"`).
    pub cliff_class: String,
    /// Texture directory (e.g. `"ReplaceableTextures\\Cliff"`).
    pub tex_dir: String,
    /// Texture file name (e.g. `"Cliff0"`).
    pub tex_file: String,
    /// Full resolved texture path (e.g. `"{GAME}\\War3Patch.mpq\\ReplaceableTextures\\Cliff\\Cliff0.blp"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tex_source: Option<String>,
    /// Ground tile rawcode override near cliffs (e.g. `"Ldrt"`).
    pub ground_tile: String,
}

/// Result of loading `TerrainArt\CliffTypes.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliffTypesSlkResult {
    /// Where the file was found.
    pub source: String,
    /// Cliff types keyed by `cliffID` rawcode string (e.g. `"CLdi"`).
    pub cliff_types: HashMap<String, CliffTypeInfo>,
}

/// Try to load and parse `TerrainArt\CliffTypes.slk` via the cascading lookup.
/// When `tileset` is `Some("Y")`, the lookup will search `Y.mpq` for cliff textures.
pub fn load_cliff_types_slk(archive_path: Option<&str>, tileset: Option<&str>) -> Option<CliffTypesSlkResult> {
    let effective_tileset = tileset
        .map(|s| s.to_string())
        .or_else(|| crate::lng::w3e::game_path::get_tileset());
    log::info!("load_cliff_types_slk: looking for TerrainArt\\CliffTypes.slk (archive={:?}, tileset={:?})", archive_path, effective_tileset);
    let (buf, source) = crate::lng::w3e::file_lookup::lookup_file(
        "TerrainArt\\CliffTypes.slk",
        archive_path,
    )?;

    let rows = parse_slk(&buf);

    let mut cliff_types = HashMap::new();
    for row in rows {
        let cliff_id = match row.get("cliffID") {
            Some(id) if !id.is_empty() => id.clone(),
            _ => continue,
        };

        let tex_dir = row.get("texDir").cloned().unwrap_or_default();
        let tex_file = row.get("texFile").cloned().unwrap_or_default();

        // Resolve where the texture file actually lives (using explicit tileset)
        let tex_source = if !tex_dir.is_empty() && !tex_file.is_empty() {
            let tex_path = format!("{}\\{}.blp", tex_dir, tex_file);
            let result = crate::lng::w3e::file_lookup::lookup_file_ext(&tex_path, archive_path, effective_tileset.as_deref())
                .map(|(_buf, src)| src);
            log::info!("load_cliff_types_slk: cliff={} tex={} tileset={:?} => {:?}", cliff_id, tex_path, effective_tileset, result);
            result
        } else {
            None
        };

        cliff_types.insert(cliff_id.clone(), CliffTypeInfo {
            cliff_id,
            cliff_model_dir: row.get("cliffModelDir").cloned().unwrap_or_default(),
            ramp_model_dir: row.get("rampModelDir").cloned().unwrap_or_default(),
            cliff_class: row.get("cliffClass").cloned().unwrap_or_default(),
            tex_dir,
            tex_file,
            tex_source,
            ground_tile: row.get("groundTile").cloned().unwrap_or_default(),
        });
    }

    log::info!("load_cliff_types_slk: found {} cliff types in '{}' ({} bytes)", cliff_types.len(), source, buf.len());
    Some(CliffTypesSlkResult {
        source: source.to_string(),
        cliff_types,
    })
}

// ─── Cliff model variations ──────────────────────────────────────────────────

/// Max variation index per cliff letter-pattern (e.g. `"CBAA" → 0`).
///
/// HiveWE stores these in `data/warcraft/Cliffs.slk` and `CityCliffs.slk`.
/// The cliff_variation value is clamped to `[0, max]` at placement time
/// (HiveWE terrain.ixx line 1080).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliffVariationsResult {
    /// Pattern → max variation for regular `Cliffs` directory.
    pub cliffs: HashMap<String, u32>,
    /// Pattern → max variation for `CityCliffs` directory.
    pub city_cliffs: HashMap<String, u32>,
}

/// Parse an embedded cliff-variations SLK (`cliffID`, `variations` columns)
/// into a `pattern → max_variation` map.
fn parse_cliff_variations_slk(data: &[u8]) -> HashMap<String, u32> {
    let rows = parse_slk(data);
    let mut map = HashMap::new();
    for row in rows {
        if let Some(id) = row.get("cliffID") {
            if !id.is_empty() {
                let v: u32 = row.get("variations")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                map.insert(id.clone(), v);
            }
        }
    }
    map
}

/// Build the cliff-variations data from embedded SLK fixtures.
pub fn load_cliff_variations() -> CliffVariationsResult {
    log::info!("load_cliff_variations: parsing embedded Cliffs.slk and CityCliffs.slk fixtures");
    let cliffs = parse_cliff_variations_slk(
        include_bytes!("../../../lng/slk/fixtures/Doodads/Terrain/Cliffs.slk"),
    );
    let city_cliffs = parse_cliff_variations_slk(
        include_bytes!("../../../lng/slk/fixtures/Doodads/Terrain/CityCliffs.slk"),
    );
    log::info!("load_cliff_variations: {} cliff patterns, {} city cliff patterns", cliffs.len(), city_cliffs.len());
    CliffVariationsResult { cliffs, city_cliffs }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cliff_variations_embedded() {
        let result = load_cliff_variations();

        // Cliffs.slk: 64 entries
        assert!(!result.cliffs.is_empty(), "cliffs should not be empty");
        assert_eq!(result.cliffs.get("AAAB"), Some(&1));
        assert_eq!(result.cliffs.get("AABB"), Some(&2));
        assert_eq!(result.cliffs.get("AABC"), Some(&0));
        assert_eq!(result.cliffs.get("CCCA"), Some(&1));

        // CityCliffs.slk: 64 entries
        assert!(!result.city_cliffs.is_empty(), "city_cliffs should not be empty");
        assert_eq!(result.city_cliffs.get("AAAB"), Some(&2));
        assert_eq!(result.city_cliffs.get("AABB"), Some(&3));
        assert_eq!(result.city_cliffs.get("AABC"), Some(&0));
        assert_eq!(result.city_cliffs.get("CCCA"), Some(&1));
    }
}

