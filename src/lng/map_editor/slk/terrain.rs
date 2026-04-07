//! Terrain tile data from `TerrainArt\Terrain.slk`.

use serde::Serialize;
use super::parse_slk;

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
    log::info!("load_terrain_slk: looking for TerrainArt\\Terrain.slk (archive={:?})", archive_path);
    let (buf, source) = crate::lng::map_editor::file_lookup::lookup_file(
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
                if crate::lng::map_editor::file_lookup::lookup_file_exists(
                    &format!("{}.tga", base),
                    archive_path,
                ) {
                    ".tga".to_string()
                } else if crate::lng::map_editor::file_lookup::lookup_file_exists(
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

    log::info!("load_terrain_slk: found {} tiles in '{}' ({} bytes)", tiles.len(), source, buf.len());
    Some(TerrainSlkResult {
        source: source.to_string(),
        tiles,
    })
}

