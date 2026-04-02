//! Generic SYLK (`.slk`) parser and terrain tile metadata loader.
//!
//! The parser reads raw SLK bytes and returns rows as `Vec<HashMap<String, String>>`.
//! Then `load_terrain_slk` uses the cascading file lookup to find
//! `TerrainArt\Terrain.slk` and extracts the columns we need.

use serde::Serialize;
use std::collections::HashMap;
use super::westrings::GameString;


// ─── Color ───────────────────────────────────────────────────────────────────

/// RGBA colour.
#[derive(Debug, Clone, Serialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

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

// ─── Doodad ──────────────────────────────────────────────────────────────────

/// Helper: parse an SLK field as `u8`, defaulting to `def`.
fn slk_u8(row: &HashMap<String, String>, key: &str, def: u8) -> u8 {
    row.get(key).and_then(|v| v.parse().ok()).unwrap_or(def)
}

/// Helper: parse an SLK field as `u32`, defaulting to `def`.
fn slk_u32(row: &HashMap<String, String>, key: &str, def: u32) -> u32 {
    row.get(key).and_then(|v| v.parse().ok()).unwrap_or(def)
}

/// Helper: parse an SLK field as `f64`, defaulting to `def`.
fn slk_f64(row: &HashMap<String, String>, key: &str, def: f64) -> f64 {
    row.get(key).and_then(|v| v.parse().ok()).unwrap_or(def)
}

/// Helper: parse an SLK field as boolean (`"1"` = true).
fn slk_bool(row: &HashMap<String, String>, key: &str) -> bool {
    row.get(key).map(|v| v == "1").unwrap_or(false)
}

/// Helper: read an SLK string, returning empty for `"_"`, `"-"`, or missing.
fn slk_str(row: &HashMap<String, String>, key: &str) -> String {
    row.get(key)
        .filter(|v| *v != "_" && *v != "-")
        .cloned()
        .unwrap_or_default()
}

/// A single doodad entry extracted from `Doodads\Doodads.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Doodad {
    /// Rawcode text, e.g. `"APms"`.
    pub dood_id: String,
    pub name: GameString,
    pub comment: String,
    pub category: String,
    pub tilesets: String,
    pub tileset_specific: bool,
    pub file: String,
    pub dood_class: String,
    pub sound_loop: String,
    pub num_var: u32,
    pub def_scale: f64,
    pub min_scale: f64,
    pub max_scale: f64,
    pub can_place_rand_scale: bool,
    pub sel_size: f64,
    pub use_click_helper: bool,
    pub ignore_model_click: bool,
    pub max_pitch: f64,
    pub max_roll: f64,
    pub vis_radius: f64,
    pub walkable: bool,
    pub on_cliffs: bool,
    pub on_water: bool,
    pub floats: bool,
    pub shadow: bool,
    pub show_in_fog: bool,
    pub anim_in_fog: bool,
    pub fixed_rot: f64,
    pub path_tex: String,
    pub show_in_mm: bool,
    pub use_mm_color: bool,
    /// Minimap colour (from `MMRed`, `MMGreen`, `MMBlue`).
    pub mm_color: Color,
    /// Per-variation vertex colours (from `vertR01..10`, `vertG01..10`, `vertB01..10`).
    pub vert_colors: Vec<Color>,
    pub in_beta: bool,
    pub version: u32,
}

/// Result of loading `Doodads\Doodads.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoodadsSlkResult {
    /// Where the file was found (e.g. `"War3Patch.mpq"`, `"game folder"`, …).
    pub source: String,
    /// Doodads keyed by rawcode `u32` (little-endian interpretation of the
    /// 4-byte doodID).
    pub doodads: HashMap<u32, Doodad>,
}

/// Convert a 4-char SLK doodID string to its rawcode `u32` key.
fn dood_id_to_u32(id: &str) -> u32 {
    let bytes = id.as_bytes();
    let mut b = [0u8; 4];
    for (i, &byte) in bytes.iter().take(4).enumerate() {
        b[i] = byte;
    }
    u32::from_le_bytes(b)
}

/// Try to load and parse `Doodads\Doodads.slk` via the cascading lookup.
pub fn load_doodads_slk(archive_path: Option<&str>) -> Option<DoodadsSlkResult> {
    let (buf, source) = super::file_lookup::lookup_file(
        "Doodads\\Doodads.slk",
        archive_path,
    )?;

    // Ensure WorldEditStrings are loaded for WESTRING_* resolution.
    super::westrings::ensure_loaded(archive_path);

    let rows = parse_slk(&buf);

    let mut doodads = HashMap::new();
    for row in rows {
        let dood_id = match row.get("doodID") {
            Some(id) if !id.is_empty() => id.clone(),
            _ => continue,
        };

        // Resolve WESTRING_* references in the Name field.
        let raw_name = row.get("Name").cloned().unwrap_or_default();
        let name = super::westrings::resolve_game_string(&raw_name);

        // Minimap colour
        let mm_color = Color::rgb(
            slk_u8(&row, "MMRed", 0),
            slk_u8(&row, "MMGreen", 0),
            slk_u8(&row, "MMBlue", 0),
        );

        // Vertex colours (variations 01..10)
        let mut vert_colors = Vec::new();
        for i in 1..=10 {
            let idx = format!("{:02}", i);
            let r_key = format!("vertR{}", idx);
            let g_key = format!("vertG{}", idx);
            let b_key = format!("vertB{}", idx);
            // Only include if at least one component is present.
            if row.contains_key(&r_key) || row.contains_key(&g_key) || row.contains_key(&b_key) {
                vert_colors.push(Color::rgb(
                    slk_u8(&row, &r_key, 255),
                    slk_u8(&row, &g_key, 255),
                    slk_u8(&row, &b_key, 255),
                ));
            }
        }

        let key = dood_id_to_u32(&dood_id);
        doodads.insert(key, Doodad {
            dood_id,
            name,
            comment: row.get("comment").cloned().unwrap_or_default(),
            category: row.get("category").cloned().unwrap_or_default(),
            tilesets: row.get("tilesets").cloned().unwrap_or_default(),
            tileset_specific: slk_bool(&row, "tilesetSpecific"),
            file: row.get("file").cloned().unwrap_or_default(),
            dood_class: slk_str(&row, "doodClass"),
            sound_loop: slk_str(&row, "soundLoop"),
            num_var: slk_u32(&row, "numVar", 1),
            def_scale: slk_f64(&row, "defScale", 1.0),
            min_scale: slk_f64(&row, "minScale", 0.0),
            max_scale: slk_f64(&row, "maxScale", 0.0),
            can_place_rand_scale: slk_bool(&row, "canPlaceRandScale"),
            sel_size: slk_f64(&row, "selSize", 0.0),
            use_click_helper: slk_bool(&row, "useClickHelper"),
            ignore_model_click: slk_bool(&row, "ignoreModelClick"),
            max_pitch: slk_f64(&row, "maxPitch", -1.0),
            max_roll: slk_f64(&row, "maxRoll", -1.0),
            vis_radius: slk_f64(&row, "visRadius", 0.0),
            walkable: slk_bool(&row, "walkable"),
            on_cliffs: slk_bool(&row, "onCliffs"),
            on_water: slk_bool(&row, "onWater"),
            floats: slk_bool(&row, "floats"),
            shadow: slk_bool(&row, "shadow"),
            show_in_fog: slk_bool(&row, "showInFog"),
            anim_in_fog: slk_bool(&row, "animInFog"),
            fixed_rot: slk_f64(&row, "fixedRot", -1.0),
            path_tex: slk_str(&row, "pathTex"),
            show_in_mm: slk_bool(&row, "showInMM"),
            use_mm_color: slk_bool(&row, "useMMColor"),
            mm_color,
            vert_colors,
            in_beta: slk_bool(&row, "InBeta"),
            version: slk_u32(&row, "version", 0),
        });
    }

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
    /// Model file path from `Units\unitUI.slk` (`file` column).
    pub file: String,
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
///
/// Also attempts to load `Units\unitUI.slk` and merge the `file` (model path)
/// column into each [`UnitInfo`].
pub fn load_units_slk(archive_path: Option<&str>) -> Option<UnitsSlkResult> {
    let (buf, source) = super::file_lookup::lookup_file(
        "Units\\UnitData.slk",
        archive_path,
    )?;

    let rows = parse_slk(&buf);

    // Load unitUI.slk to get model file paths keyed by unit ID.
    let ui_file_map: HashMap<String, String> = super::file_lookup::lookup_file(
        "Units\\unitUI.slk",
        archive_path,
    )
    .map(|(ui_buf, _)| {
        let ui_rows = parse_slk(&ui_buf);
        ui_rows
            .into_iter()
            .filter_map(|row| {
                let uid = row.get("unitUIID")?.clone();
                let file = row.get("file").cloned().unwrap_or_default();
                if uid.is_empty() { None } else { Some((uid, file)) }
            })
            .collect()
    })
    .unwrap_or_default();

    let units: Vec<UnitInfo> = rows
        .into_iter()
        .filter_map(|row| {
            let unit_id = row.get("unitID")?.clone();
            if unit_id.is_empty() {
                return None;
            }
            let file = ui_file_map.get(&unit_id).cloned().unwrap_or_default();
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
                file,
            })
        })
        .collect();

    Some(UnitsSlkResult {
        source: source.to_string(),
        units,
    })
}

// ─── Destructable ────────────────────────────────────────────────────────────

/// A single destructable entry extracted from `Units\DestructableData.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Destructable {
    /// Rawcode text, e.g. `"ATtr"`.
    pub destructable_id: String,
    pub name: GameString,
    pub editor_suffix: GameString,
    pub comment: GameString,
    pub category: String,
    pub tilesets: String,
    pub tileset_specific: bool,
    pub file: String,
    pub lightweight: bool,
    pub fat_los: bool,
    pub tex_id: u32,
    pub tex_file: String,
    pub dood_class: String,
    pub use_click_helper: bool,
    pub on_cliffs: bool,
    pub on_water: bool,
    pub can_place_dead: bool,
    pub walkable: bool,
    pub cliff_height: u32,
    pub targ_type: String,
    pub armor: String,
    pub num_var: u32,
    pub hp: u32,
    pub occ_h: f64,
    pub fly_h: f64,
    pub fixed_rot: f64,
    pub sel_size: f64,
    pub min_scale: f64,
    pub max_scale: f64,
    pub can_place_rand_scale: bool,
    pub max_pitch: f64,
    pub max_roll: f64,
    pub radius: f64,
    pub fog_radius: f64,
    pub fog_vis: bool,
    pub path_tex: String,
    pub path_tex_death: String,
    pub death_snd: String,
    pub shadow: bool,
    /// Tint colour (from `colorR`, `colorG`, `colorB`).
    pub color: Color,
    pub show_in_mm: bool,
    pub use_mm_color: bool,
    /// Minimap colour (from `MMRed`, `MMGreen`, `MMBlue`).
    pub mm_color: Color,
    pub build_time: u32,
    pub repair_time: u32,
    pub gold_rep: u32,
    pub lumber_rep: u32,
    pub in_beta: bool,
    pub version: u32,
    pub selectable: bool,
    pub selcircsize: f64,
    pub portraitmodel: String,
}

/// Result of loading `Units\DestructableData.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestructablesSlkResult {
    /// Where the file was found (e.g. `"War3Patch.mpq"`, `"game folder"`, …).
    pub source: String,
    /// Destructables keyed by rawcode `u32` (little-endian interpretation of
    /// the 4-byte DestructableID).
    pub destructables: HashMap<u32, Destructable>,
}

/// Convert a 4-char SLK DestructableID string to its rawcode `u32` key.
fn dest_id_to_u32(id: &str) -> u32 {
    let bytes = id.as_bytes();
    let mut b = [0u8; 4];
    for (i, &byte) in bytes.iter().take(4).enumerate() {
        b[i] = byte;
    }
    u32::from_le_bytes(b)
}

/// Try to load and parse `Units\DestructableData.slk` via the cascading lookup.
pub fn load_destructables_slk(archive_path: Option<&str>) -> Option<DestructablesSlkResult> {
    let (buf, source) = super::file_lookup::lookup_file(
        "Units\\DestructableData.slk",
        archive_path,
    )?;

    // Ensure WorldEditStrings are loaded for WESTRING_* resolution.
    super::westrings::ensure_loaded(archive_path);

    let rows = parse_slk(&buf);

    let mut destructables = HashMap::new();
    for row in rows {
        let destructable_id = match row.get("DestructableID") {
            Some(id) if !id.is_empty() => id.clone(),
            _ => continue,
        };

        // Resolve WESTRING_* references in the Name field.
        let raw_name = row.get("Name").cloned().unwrap_or_default();
        let name = super::westrings::resolve_game_string(&raw_name);

        // Resolve WESTRING_* references in EditorSuffix.
        let raw_suffix = row.get("EditorSuffix")
            .filter(|v| *v != "_" && *v != "-")
            .cloned()
            .unwrap_or_default();
        let editor_suffix = super::westrings::resolve_game_string(&raw_suffix);

        // Resolve WESTRING_* references in comment.
        let raw_comment = row.get("comment").cloned().unwrap_or_default();
        let comment = super::westrings::resolve_game_string(&raw_comment);

        // Tint colour
        let color = Color::rgb(
            slk_u8(&row, "colorR", 255),
            slk_u8(&row, "colorG", 255),
            slk_u8(&row, "colorB", 255),
        );

        // Minimap colour
        let mm_color = Color::rgb(
            slk_u8(&row, "MMRed", 0),
            slk_u8(&row, "MMGreen", 0),
            slk_u8(&row, "MMBlue", 0),
        );

        let key = dest_id_to_u32(&destructable_id);
        destructables.insert(key, Destructable {
            destructable_id,
            name,
            editor_suffix,
            comment,
            category: row.get("category").cloned().unwrap_or_default(),
            tilesets: row.get("tilesets").cloned().unwrap_or_default(),
            tileset_specific: slk_bool(&row, "tilesetSpecific"),
            file: row.get("file").cloned().unwrap_or_default(),
            lightweight: slk_bool(&row, "lightweight"),
            fat_los: slk_bool(&row, "fatLOS"),
            tex_id: slk_u32(&row, "texID", 0),
            tex_file: slk_str(&row, "texFile"),
            dood_class: slk_str(&row, "doodClass"),
            use_click_helper: slk_bool(&row, "useClickHelper"),
            on_cliffs: slk_bool(&row, "onCliffs"),
            on_water: slk_bool(&row, "onWater"),
            can_place_dead: slk_bool(&row, "canPlaceDead"),
            walkable: slk_bool(&row, "walkable"),
            cliff_height: slk_u32(&row, "cliffHeight", 0),
            targ_type: row.get("targType").cloned().unwrap_or_default(),
            armor: row.get("armor").cloned().unwrap_or_default(),
            num_var: slk_u32(&row, "numVar", 1),
            hp: slk_u32(&row, "HP", 0),
            occ_h: slk_f64(&row, "occH", 0.0),
            fly_h: slk_f64(&row, "flyH", 0.0),
            fixed_rot: slk_f64(&row, "fixedRot", -1.0),
            sel_size: slk_f64(&row, "selSize", 0.0),
            min_scale: slk_f64(&row, "minScale", 0.0),
            max_scale: slk_f64(&row, "maxScale", 0.0),
            can_place_rand_scale: slk_bool(&row, "canPlaceRandScale"),
            max_pitch: slk_f64(&row, "maxPitch", -1.0),
            max_roll: slk_f64(&row, "maxRoll", -1.0),
            radius: slk_f64(&row, "radius", 0.0),
            fog_radius: slk_f64(&row, "fogRadius", 0.0),
            fog_vis: slk_bool(&row, "fogVis"),
            path_tex: slk_str(&row, "pathTex"),
            path_tex_death: slk_str(&row, "pathTexDeath"),
            death_snd: slk_str(&row, "deathSnd"),
            shadow: slk_bool(&row, "shadow"),
            color,
            show_in_mm: slk_bool(&row, "showInMM"),
            use_mm_color: slk_bool(&row, "useMMColor"),
            mm_color,
            build_time: slk_u32(&row, "buildTime", 0),
            repair_time: slk_u32(&row, "repairTime", 0),
            gold_rep: slk_u32(&row, "goldRep", 0),
            lumber_rep: slk_u32(&row, "lumberRep", 0),
            in_beta: slk_bool(&row, "InBeta"),
            version: slk_u32(&row, "version", 0),
            selectable: slk_bool(&row, "selectable"),
            selcircsize: slk_f64(&row, "selcircsize", 0.0),
            portraitmodel: slk_str(&row, "portraitmodel"),
        });
    }

    Some(DestructablesSlkResult {
        source: source.to_string(),
        destructables,
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

    /// Collect all unique `category` values, tileset characters, and doodad
    /// names from `Doodads\Doodads.slk` so we can use them for UI filters.
    ///
    /// Run manually:
    /// ```sh
    /// cargo test --package JASS-Tree-sitter-Rust w3e::slk::tests::dump_doodad_categories_and_tilesets -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_doodad_categories_and_tilesets() {
        use std::collections::BTreeMap;

        let data = include_bytes!("../../lng/slk/fixtures/Doodads/Doodads.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some doodad rows");

        let mut categories: BTreeMap<String, usize> = BTreeMap::new();
        let mut tilesets: BTreeMap<char, usize> = BTreeMap::new();
        let mut names: Vec<(String, String, String, String)> = Vec::new(); // (doodID, Name, category, tilesets)

        for row in &rows {
            let dood_id = row.get("doodID").cloned().unwrap_or_default();
            if dood_id.is_empty() {
                continue;
            }

            // Category
            let cat = row.get("category").cloned().unwrap_or_default();
            if !cat.is_empty() {
                *categories.entry(cat.clone()).or_insert(0) += 1;
            }

            // Tilesets – each character is a separate tileset code
            let ts = row.get("tilesets").cloned().unwrap_or_default();
            for ch in ts.chars() {
                *tilesets.entry(ch).or_insert(0) += 1;
            }

            // Name
            let name = row.get("Name").cloned().unwrap_or_default();
            names.push((dood_id, name, cat, ts));
        }

        println!("\n══════════════════════════════════════════");
        println!("  Doodads.slk — {} entries", rows.len());
        println!("══════════════════════════════════════════\n");

        println!("── Categories ({}) ──", categories.len());
        for (cat, count) in &categories {
            println!("  {:<20} {:>4} doodads", cat, count);
        }

        println!("\n── Tileset characters ({}) ──", tilesets.len());
        for (ch, count) in &tilesets {
            println!("  '{}' {:>5} doodads", ch, count);
        }

        println!("\n── Doodad names (first 30) ──");
        for (id, name, cat, ts) in names.iter().take(30) {
            println!("  {} | {:<40} | cat={:<16} | ts={}", id, name, cat, ts);
        }

        println!("\nTotal categories: {}", categories.len());
        println!("Total tileset chars: {}", tilesets.len());
        println!("Total doodad entries: {}", rows.len());
    }

    /// Dump all column names from `Doodads.slk` into a text file next to the fixture.
    ///
    /// Run manually:
    /// ```sh
    /// cargo test --package JASS-Tree-sitter-Rust w3e::slk::tests::dump_doodad_field_names -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_doodad_field_names() {
        use std::collections::BTreeSet;

        let data = include_bytes!("../../lng/slk/fixtures/Doodads/Doodads.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some doodad rows");

        let mut fields = BTreeSet::new();
        for row in &rows {
            for key in row.keys() {
                fields.insert(key.clone());
            }
        }

        let out_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/lng/slk/fixtures/Doodads/field_names.txt"
        );
        let content = fields.iter().cloned().collect::<Vec<_>>().join("\n");
        std::fs::write(out_path, &content).expect("failed to write field_names.txt");

        println!("\nWrote {} field names to {}", fields.len(), out_path);
        for f in &fields {
            println!("  {}", f);
        }
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

    #[test]
    fn parse_destructable_slk_fixture() {
        let data = include_bytes!("../../lng/slk/fixtures/Units/DestructableData.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some rows");

        let first = &rows[0];
        assert_eq!(first.get("DestructableID").map(|s| s.as_str()), Some("ATtr"));
        assert_eq!(first.get("comment").map(|s| s.as_str()), Some("Tree Wall"));
        assert_eq!(first.get("category").map(|s| s.as_str()), Some("D"));
    }

    /// Collect all unique `category` values, tileset characters, and destructable
    /// names from `Units\DestructableData.slk` so we can use them for UI filters.
    ///
    /// Run manually:
    /// ```sh
    /// cargo test --package JASS-Tree-sitter-Rust w3e::slk::tests::dump_destructable_categories_and_tilesets -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_destructable_categories_and_tilesets() {
        use std::collections::BTreeMap;

        let data = include_bytes!("../../lng/slk/fixtures/Units/DestructableData.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some destructable rows");

        let mut categories: BTreeMap<String, usize> = BTreeMap::new();
        let mut tilesets: BTreeMap<char, usize> = BTreeMap::new();
        let mut names: Vec<(String, String, String, String)> = Vec::new(); // (DestructableID, Name, category, tilesets)

        for row in &rows {
            let dest_id = row.get("DestructableID").cloned().unwrap_or_default();
            if dest_id.is_empty() {
                continue;
            }

            // Category
            let cat = row.get("category").cloned().unwrap_or_default();
            if !cat.is_empty() {
                *categories.entry(cat.clone()).or_insert(0) += 1;
            }

            // Tilesets – each character is a separate tileset code
            let ts = row.get("tilesets").cloned().unwrap_or_default();
            for ch in ts.chars() {
                *tilesets.entry(ch).or_insert(0) += 1;
            }

            // Name
            let name = row.get("Name").cloned().unwrap_or_default();
            names.push((dest_id, name, cat, ts));
        }

        println!("\n══════════════════════════════════════════");
        println!("  DestructableData.slk — {} entries", rows.len());
        println!("══════════════════════════════════════════\n");

        println!("── Categories ({}) ──", categories.len());
        for (cat, count) in &categories {
            println!("  {:<20} {:>4} destructables", cat, count);
        }

        println!("\n── Tileset characters ({}) ──", tilesets.len());
        for (ch, count) in &tilesets {
            println!("  '{}' {:>5} destructables", ch, count);
        }

        println!("\n── Destructable names (first 30) ──");
        for (id, name, cat, ts) in names.iter().take(30) {
            println!("  {} | {:<40} | cat={:<16} | ts={}", id, name, cat, ts);
        }

        println!("\nTotal categories: {}", categories.len());
        println!("Total tileset chars: {}", tilesets.len());
        println!("Total destructable entries: {}", rows.len());
    }

    /// Dump all column names from `DestructableData.slk` into a text file next to the fixture.
    ///
    /// Run manually:
    /// ```sh
    /// cargo test --package JASS-Tree-sitter-Rust w3e::slk::tests::dump_destructable_field_names -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_destructable_field_names() {
        use std::collections::BTreeSet;

        let data = include_bytes!("../../lng/slk/fixtures/Units/DestructableData.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some destructable rows");

        let mut fields = BTreeSet::new();
        for row in &rows {
            for key in row.keys() {
                fields.insert(key.clone());
            }
        }

        let out_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/lng/slk/fixtures/Units/destructable_field_names.txt"
        );
        let content = fields.iter().cloned().collect::<Vec<_>>().join("\n");
        std::fs::write(out_path, &content).expect("failed to write destructable_field_names.txt");

        println!("\nWrote {} field names to {}", fields.len(), out_path);
        for f in &fields {
            println!("  {}", f);
        }
    }
}

