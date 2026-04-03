//! Generic SYLK (`.slk`) parser and terrain tile metadata loader.
//!
//! The parser reads raw SLK bytes and returns rows as `Vec<HashMap<String, String>>`.
//! Then `load_terrain_slk` uses the cascading file lookup to find
//! `TerrainArt\Terrain.slk` and extracts the columns we need.

use serde::Serialize;
use std::collections::HashMap;
use super::westrings::GameString;
use tree_sitter::Parser;
use crate::lng::bni::kind::Kind;


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

// ─── INI-style UnitStrings parser ─────────────────────────────────────────────

/// Parse an INI-format UnitStrings.txt file into a map of section rawcodes to
/// field key/value maps.
///
/// Format:
/// ```text
/// [Hamg]
/// Name=Archmage
/// Tip=Summon |cffffcc00A|rrchmage
/// Ubertip="Mystical Hero, adept at ranged assaults..."
/// ```
///
/// Returns `HashMap<"Hamg", {"Name": "Archmage", "Tip": "Summon ...", ...}>`.
pub fn parse_unit_strings(data: &[u8]) -> HashMap<String, HashMap<String, String>> {
    let text = String::from_utf8_lossy(data);
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bni::LANGUAGE.into())
        .expect("Failed to set BNI language");

    let Some(tree) = parser.parse(text.as_bytes(), None) else {
        return HashMap::new();
    };

    let mut result: HashMap<String, HashMap<String, String>> = HashMap::new();
    let root = tree.root_node();
    let mut current_section: Option<String> = None;

    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        let Ok(kind) = Kind::try_from(node.grammar_id()) else {
            continue;
        };
        match kind {
            Kind::Section => {
                // Extract section name from children
                let mut sc = node.walk();
                for child in node.children(&mut sc) {
                    if Kind::try_from(child.grammar_id()) == Ok(Kind::SectionName) {
                        if let Ok(name) = child.utf8_text(text.as_bytes()) {
                            current_section = Some(name.to_string());
                            result.entry(name.to_string()).or_default();
                        }
                        break;
                    }
                }
            }
            Kind::Item => {
                let Some(ref section) = current_section else { continue };

                let mut key: Option<&str> = None;
                let mut value = String::new();

                let mut child_cursor = node.walk();
                for child in node.children(&mut child_cursor) {
                    let Ok(ck) = Kind::try_from(child.grammar_id()) else {
                        continue;
                    };
                    match ck {
                        Kind::Key => {
                            key = child.utf8_text(text.as_bytes()).ok();
                        }
                        Kind::ValueList => {
                            let mut val_cursor = child.walk();
                            for val_child in child.children(&mut val_cursor) {
                                let Ok(vk) = Kind::try_from(val_child.grammar_id()) else {
                                    continue;
                                };
                                match vk {
                                    Kind::QuotedString => {
                                        let mut qs_cursor = val_child.walk();
                                        for qs_child in val_child.children(&mut qs_cursor) {
                                            if Kind::try_from(qs_child.grammar_id())
                                                == Ok(Kind::StringContent)
                                            {
                                                value = qs_child
                                                    .utf8_text(text.as_bytes())
                                                    .unwrap_or_default()
                                                    .to_string();
                                                break;
                                            }
                                        }
                                        break;
                                    }
                                    Kind::UnquotedString | Kind::Int | Kind::Float => {
                                        value = val_child
                                            .utf8_text(text.as_bytes())
                                            .unwrap_or_default()
                                            .to_string();
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }

                if let Some(k) = key {
                    if !k.is_empty() {
                        result.get_mut(section).unwrap().insert(k.to_string(), value);
                    }
                }
            }
            _ => {}
        }
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

/// A single unit entry merged from `Units\UnitData.slk`, `Units\UnitBalance.slk`,
/// `Units\unitUI.slk`, and `Units\UnitWeapons.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitInfo {
    // ── Identity (UnitData) ──────────────────────────────────────
    /// Rawcode text, e.g. `"Hamg"`.
    pub unit_id: String,
    pub name: GameString,
    pub comment: String,
    pub sort: String,
    pub race: String,
    pub tilesets: String,

    // ── Movement (UnitData) ──────────────────────────────────────
    pub move_tp: String,
    pub move_height: f64,
    pub move_floor: f64,
    pub turn_rate: f64,
    pub prop_win: f64,
    pub formation: u32,
    pub path_tex: String,

    // ── Combat basics (UnitData) ─────────────────────────────────
    pub targ_type: String,
    pub threat: u32,
    pub points: u32,
    pub death: f64,
    pub death_type: u32,
    pub can_sleep: bool,
    pub can_flee: bool,
    pub cargo_size: u32,
    pub prio: u32,
    pub buff_type: String,
    pub buff_radius: f64,
    pub fat_los: bool,

    // ── Stats (UnitBalance) ──────────────────────────────────────
    pub level: u32,
    pub hp: u32,
    pub real_hp: f64,
    pub regen_hp: f64,
    pub regen_type: String,
    pub mana0: u32,
    pub mana_n: u32,
    pub real_m: f64,
    pub regen_mana: f64,
    pub def: u32,
    pub def_type: String,
    pub def_up: f64,
    pub real_def: f64,
    pub spd: u32,
    pub min_spd: u32,
    pub max_spd: u32,
    pub sight: u32,
    pub nsight: u32,
    pub bld_tm: u32,
    pub rep_tm: u32,
    pub collision: f64,
    pub primary: String,
    pub str_: u32,
    pub str_plus: f64,
    pub agi: u32,
    pub agi_plus: f64,
    pub int_: u32,
    pub int_plus: f64,
    pub is_bldg: bool,
    pub unit_type: String,

    // ── Economy (UnitBalance) ────────────────────────────────────
    pub gold_cost: u32,
    pub lumber_cost: u32,
    pub gold_rep: u32,
    pub lumber_rep: u32,
    pub fmade: u32,
    pub fused: u32,
    pub bounty_dice: u32,
    pub bounty_sides: u32,
    pub bounty_plus: u32,
    pub stock_max: u32,
    pub stock_regen: u32,
    pub stock_start: u32,

    // ── Model / UI (unitUI) ──────────────────────────────────────
    pub file: String,
    pub model_scale: f64,
    pub scale: f64,
    pub scale_bull: f64,
    pub occ_h: f64,
    pub sel_z: f64,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub team_color: i32,
    pub custom_team_color: bool,
    pub unit_sound: String,
    pub unit_class: String,
    pub special: String,
    pub unit_shadow: String,
    pub building_shadow: String,
    pub shadow_on_water: bool,
    pub sel_circ_on_water: bool,
    pub max_pitch: f64,
    pub max_roll: f64,
    pub elev_pts: u32,
    pub elev_rad: f64,
    pub fog_rad: f64,
    pub uber_splat: String,
    pub in_editor: bool,
    pub hidden_in_editor: bool,

    // ── Weapons (UnitWeapons) ────────────────────────────────────
    pub weaps_on: u32,
    pub acquire: f64,
    // Weapon 1
    pub weap_tp1: String,
    pub weap_type1: String,
    pub atk_type1: String,
    pub cool1: f64,
    pub dmgplus1: u32,
    pub dice1: u32,
    pub sides1: u32,
    pub range_n1: f64,
    pub targs1: String,
    pub show_ui1: bool,
    pub dmg_pt1: f64,
    pub back_sw1: f64,
    pub splash_targs1: String,
    pub min_range: f64,
    // Weapon 2
    pub weap_tp2: String,
    pub weap_type2: String,
    pub atk_type2: String,
    pub cool2: f64,
    pub dmgplus2: u32,
    pub dice2: u32,
    pub sides2: u32,
    pub range_n2: f64,
    pub targs2: String,
    pub show_ui2: bool,
    pub dmg_pt2: f64,
    pub back_sw2: f64,
    pub splash_targs2: String,

    // ── Meta ─────────────────────────────────────────────────────
    pub in_beta: bool,
    pub version: u32,

    // ── Strings (from *UnitStrings.txt) ──────────────────────────
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ubertip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub propernames: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revivetip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub awakentip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caster_upgrade_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caster_upgrade_tip: Option<String>,
}

/// Per-SLK source info.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlkSource {
    /// SLK file name, e.g. `"UnitData.slk"`.
    pub name: String,
    /// Where the file was found, e.g. `"War3Patch.mpq"`.
    pub source: String,
    /// Number of rows parsed.
    pub rows: usize,
}

/// Result of loading unit SLK files.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitsSlkResult {
    /// Where the primary file was found.
    pub source: String,
    /// Per-file source info for all loaded SLK files.
    pub sources: Vec<SlkSource>,
    /// Units keyed by rawcode `u32` (little-endian interpretation of the
    /// 4-byte unitID).
    pub units: HashMap<u32, UnitInfo>,
}

/// Convert a 4-char SLK unitID string to its rawcode `u32` key.
fn unit_id_to_u32(id: &str) -> u32 {
    let bytes = id.as_bytes();
    let mut b = [0u8; 4];
    for (i, &byte) in bytes.iter().take(4).enumerate() {
        b[i] = byte;
    }
    u32::from_le_bytes(b)
}

/// Helper: build a `HashMap<String, HashMap<String,String>>` from an SLK
/// parsed into rows, keyed by the given ID column.
fn slk_index_by(rows: Vec<HashMap<String, String>>, id_col: &str) -> HashMap<String, HashMap<String, String>> {
    let mut map = HashMap::new();
    for row in rows {
        if let Some(id) = row.get(id_col).filter(|v| !v.is_empty()) {
            map.insert(id.clone(), row);
        }
    }
    map
}

/// Try to load and parse unit SLK files via the cascading lookup.
///
/// Merges data from `UnitData.slk` (primary), `UnitBalance.slk`, `unitUI.slk`,
/// and `UnitWeapons.slk`.
pub fn load_units_slk(archive_path: Option<&str>) -> Option<UnitsSlkResult> {
    let (buf, source) = super::file_lookup::lookup_file(
        "Units\\UnitData.slk",
        archive_path,
    )?;

    // Ensure WorldEditStrings are loaded for WESTRING_* resolution.
    super::westrings::ensure_loaded(archive_path);

    let data_rows = parse_slk(&buf);

    let mut sources = vec![SlkSource {
        name: "UnitData.slk".into(),
        source: source.to_string(),
        rows: data_rows.len(),
    }];

    // Load supplementary SLK files, indexed by their ID columns.
    let balance_map = super::file_lookup::lookup_file("Units\\UnitBalance.slk", archive_path)
        .map(|(b, src)| {
            let rows = parse_slk(&b);
            let n = rows.len();
            let indexed = slk_index_by(rows, "unitBalanceID");
            sources.push(SlkSource { name: "UnitBalance.slk".into(), source: src, rows: n });
            indexed
        })
        .unwrap_or_default();

    let ui_map = super::file_lookup::lookup_file("Units\\unitUI.slk", archive_path)
        .map(|(b, src)| {
            let rows = parse_slk(&b);
            let n = rows.len();
            let indexed = slk_index_by(rows, "unitUIID");
            sources.push(SlkSource { name: "unitUI.slk".into(), source: src, rows: n });
            indexed
        })
        .unwrap_or_default();

    let weap_map = super::file_lookup::lookup_file("Units\\UnitWeapons.slk", archive_path)
        .map(|(b, src)| {
            let rows = parse_slk(&b);
            let n = rows.len();
            let indexed = slk_index_by(rows, "unitWeapID");
            sources.push(SlkSource { name: "UnitWeapons.slk".into(), source: src, rows: n });
            indexed
        })
        .unwrap_or_default();

    let empty = HashMap::new();

    let mut units = HashMap::new();
    for row in data_rows {
        let unit_id = match row.get("unitID") {
            Some(id) if !id.is_empty() => id.clone(),
            _ => continue,
        };

        let bal = balance_map.get(&unit_id).unwrap_or(&empty);
        let ui = ui_map.get(&unit_id).unwrap_or(&empty);
        let wp = weap_map.get(&unit_id).unwrap_or(&empty);

        // Resolve WESTRING_* references in the Name field (from unitUI).
        let raw_name = ui.get("name").cloned().unwrap_or_default();
        let name = super::westrings::resolve_game_string(&raw_name);

        let key = unit_id_to_u32(&unit_id);
        units.insert(key, UnitInfo {
            unit_id,
            name,
            comment: row.get("comment(s)").cloned().unwrap_or_default(),
            sort: row.get("sort").cloned().unwrap_or_default(),
            race: row.get("race").cloned().unwrap_or_default(),
            tilesets: bal.get("tilesets").cloned().unwrap_or_default(),

            // Movement
            move_tp: row.get("movetp").cloned().unwrap_or_default(),
            move_height: slk_f64(&row, "moveHeight", 0.0),
            move_floor: slk_f64(&row, "moveFloor", 0.0),
            turn_rate: slk_f64(&row, "turnRate", 0.6),
            prop_win: slk_f64(&row, "propWin", 60.0),
            formation: slk_u32(&row, "formation", 0),
            path_tex: slk_str(&row, "pathTex"),

            // Combat basics
            targ_type: row.get("targType").cloned().unwrap_or_default(),
            threat: slk_u32(&row, "threat", 0),
            points: slk_u32(&row, "points", 0),
            death: slk_f64(&row, "death", 0.0),
            death_type: slk_u32(&row, "deathType", 0),
            can_sleep: slk_bool(&row, "canSleep"),
            can_flee: slk_bool(&row, "canFlee"),
            cargo_size: slk_u32(&row, "cargoSize", 0),
            prio: slk_u32(&row, "prio", 0),
            buff_type: slk_str(&row, "buffType"),
            buff_radius: slk_f64(&row, "buffRadius", 0.0),
            fat_los: slk_bool(&row, "fatLOS"),

            // Stats (UnitBalance)
            level: slk_u32(bal, "level", 0),
            hp: slk_u32(bal, "HP", 0),
            real_hp: slk_f64(bal, "realHP", 0.0),
            regen_hp: slk_f64(bal, "regenHP", 0.0),
            regen_type: bal.get("regenType").cloned().unwrap_or_default(),
            mana0: slk_u32(bal, "mana0", 0),
            mana_n: slk_u32(bal, "manaN", 0),
            real_m: slk_f64(bal, "realM", 0.0),
            regen_mana: slk_f64(bal, "regenMana", 0.0),
            def: slk_u32(bal, "def", 0),
            def_type: bal.get("defType").cloned().unwrap_or_default(),
            def_up: slk_f64(bal, "defUp", 0.0),
            real_def: slk_f64(bal, "realdef", 0.0),
            spd: slk_u32(bal, "spd", 0),
            min_spd: slk_u32(bal, "minSpd", 0),
            max_spd: slk_u32(bal, "maxSpd", 0),
            sight: slk_u32(bal, "sight", 0),
            nsight: slk_u32(bal, "nsight", 0),
            bld_tm: slk_u32(bal, "bldtm", 0),
            rep_tm: slk_u32(bal, "reptm", 0),
            collision: slk_f64(bal, "collision", 0.0),
            primary: bal.get("Primary").cloned().unwrap_or_default(),
            str_: slk_u32(bal, "STR", 0),
            str_plus: slk_f64(bal, "STRplus", 0.0),
            agi: slk_u32(bal, "AGI", 0),
            agi_plus: slk_f64(bal, "AGIplus", 0.0),
            int_: slk_u32(bal, "INT", 0),
            int_plus: slk_f64(bal, "INTplus", 0.0),
            is_bldg: slk_bool(bal, "isbldg"),
            unit_type: bal.get("type").cloned().unwrap_or_default(),

            // Economy (UnitBalance)
            gold_cost: slk_u32(bal, "goldcost", 0),
            lumber_cost: slk_u32(bal, "lumbercost", 0),
            gold_rep: slk_u32(bal, "goldRep", 0),
            lumber_rep: slk_u32(bal, "lumberRep", 0),
            fmade: slk_u32(bal, "fmade", 0),
            fused: slk_u32(bal, "fused", 0),
            bounty_dice: slk_u32(bal, "bountydice", 0),
            bounty_sides: slk_u32(bal, "bountysides", 0),
            bounty_plus: slk_u32(bal, "bountyplus", 0),
            stock_max: slk_u32(bal, "stockMax", 0),
            stock_regen: slk_u32(bal, "stockRegen", 0),
            stock_start: slk_u32(bal, "stockStart", 0),

            // Model / UI (unitUI)
            file: ui.get("file").cloned().unwrap_or_default(),
            model_scale: slk_f64(ui, "modelScale", 1.0),
            scale: slk_f64(ui, "scale", 1.0),
            scale_bull: slk_f64(ui, "scaleBull", 1.0),
            occ_h: slk_f64(ui, "occH", 0.0),
            sel_z: slk_f64(ui, "selZ", 0.0),
            red: slk_u8(ui, "red", 255),
            green: slk_u8(ui, "green", 255),
            blue: slk_u8(ui, "blue", 255),
            team_color: ui.get("teamColor").and_then(|v| v.parse().ok()).unwrap_or(-1),
            custom_team_color: slk_bool(ui, "customTeamColor"),
            unit_sound: slk_str(ui, "unitSound"),
            unit_class: slk_str(ui, "unitClass"),
            special: slk_str(ui, "special"),
            unit_shadow: slk_str(ui, "unitShadow"),
            building_shadow: slk_str(ui, "buildingShadow"),
            shadow_on_water: slk_bool(ui, "shadowOnWater"),
            sel_circ_on_water: slk_bool(ui, "selCircOnWater"),
            max_pitch: slk_f64(ui, "maxPitch", 0.0),
            max_roll: slk_f64(ui, "maxRoll", 0.0),
            elev_pts: slk_u32(ui, "elevPts", 0),
            elev_rad: slk_f64(ui, "elevRad", 0.0),
            fog_rad: slk_f64(ui, "fogRad", 0.0),
            uber_splat: slk_str(ui, "uberSplat"),
            in_editor: ui.get("inEditor").map(|v| v == "1").unwrap_or(true),
            hidden_in_editor: slk_bool(ui, "hiddenInEditor"),

            // Weapons (UnitWeapons)
            weaps_on: slk_u32(wp, "weapsOn", 0),
            acquire: slk_f64(wp, "acquire", 0.0),
            weap_tp1: slk_str(wp, "weapTp1"),
            weap_type1: slk_str(wp, "weapType1"),
            atk_type1: wp.get("atkType1").cloned().unwrap_or_default(),
            cool1: slk_f64(wp, "cool1", 0.0),
            dmgplus1: slk_u32(wp, "dmgplus1", 0),
            dice1: slk_u32(wp, "dice1", 0),
            sides1: slk_u32(wp, "sides1", 0),
            range_n1: slk_f64(wp, "rangeN1", 0.0),
            targs1: wp.get("targs1").cloned().unwrap_or_default(),
            show_ui1: slk_bool(wp, "showUI1"),
            dmg_pt1: slk_f64(wp, "dmgpt1", 0.0),
            back_sw1: slk_f64(wp, "backSw1", 0.0),
            splash_targs1: wp.get("splashTargs1").cloned().unwrap_or_default(),
            min_range: slk_f64(wp, "minRange", 0.0),
            weap_tp2: slk_str(wp, "weapTp2"),
            weap_type2: slk_str(wp, "weapType2"),
            atk_type2: wp.get("atkType2").cloned().unwrap_or_default(),
            cool2: slk_f64(wp, "cool2", 0.0),
            dmgplus2: slk_u32(wp, "dmgplus2", 0),
            dice2: slk_u32(wp, "dice2", 0),
            sides2: slk_u32(wp, "sides2", 0),
            range_n2: slk_f64(wp, "rangeN2", 0.0),
            targs2: wp.get("targs2").cloned().unwrap_or_default(),
            show_ui2: slk_bool(wp, "showUI2"),
            dmg_pt2: slk_f64(wp, "dmgpt2", 0.0),
            back_sw2: slk_f64(wp, "backSw2", 0.0),
            splash_targs2: wp.get("splashTargs2").cloned().unwrap_or_default(),

            // Meta
            in_beta: slk_bool(&row, "InBeta"),
            version: slk_u32(&row, "version", 0),

            // Strings (populated later from UnitStrings.txt)
            tip: None,
            ubertip: None,
            hotkey: None,
            propernames: None,
            revivetip: None,
            awakentip: None,
            editor_suffix: None,
            caster_upgrade_name: None,
            caster_upgrade_tip: None,
        });
    }

    // ── Load UnitStrings.txt files ────────────────────────────────
    const UNIT_STRING_FILES: &[&str] = &[
        "Units\\HumanUnitStrings.txt",
        "Units\\OrcUnitStrings.txt",
        "Units\\NightElfUnitStrings.txt",
        "Units\\UndeadUnitStrings.txt",
        "Units\\NeutralUnitStrings.txt",
    ];

    for &file_path in UNIT_STRING_FILES {
        if let Some((buf, src)) = super::file_lookup::lookup_file(file_path, archive_path) {
            let sections = parse_unit_strings(&buf);
            let file_name = file_path
                .rsplit('\\')
                .next()
                .unwrap_or(file_path)
                .to_string();

            sources.push(SlkSource {
                name: file_name,
                source: src,
                rows: sections.len(),
            });

            for (rawcode, fields) in sections {
                let key = unit_id_to_u32(&rawcode);
                if let Some(unit) = units.get_mut(&key) {
                    // Override name if present in UnitStrings
                    if let Some(name_val) = fields.get("Name") {
                        if !name_val.is_empty() {
                            unit.name = super::westrings::resolve_game_string(name_val);
                        }
                    }

                    fn opt(fields: &HashMap<String, String>, key: &str) -> Option<String> {
                        fields.get(key).filter(|v| !v.is_empty()).cloned()
                    }

                    if unit.tip.is_none() { unit.tip = opt(&fields, "Tip"); }
                    if unit.ubertip.is_none() { unit.ubertip = opt(&fields, "Ubertip"); }
                    if unit.hotkey.is_none() { unit.hotkey = opt(&fields, "Hotkey"); }
                    if unit.propernames.is_none() { unit.propernames = opt(&fields, "Propernames"); }
                    if unit.revivetip.is_none() { unit.revivetip = opt(&fields, "Revivetip"); }
                    if unit.awakentip.is_none() { unit.awakentip = opt(&fields, "Awakentip"); }
                    if unit.editor_suffix.is_none() { unit.editor_suffix = opt(&fields, "EditorSuffix"); }
                    if unit.caster_upgrade_name.is_none() { unit.caster_upgrade_name = opt(&fields, "Casterupgradename"); }
                    if unit.caster_upgrade_tip.is_none() { unit.caster_upgrade_tip = opt(&fields, "Casterupgradetip"); }
                }
            }
        }
    }

    Some(UnitsSlkResult {
        source: source.to_string(),
        sources,
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

    /// Dump all column names from all 4 unit SLK files.
    ///
    /// Run manually:
    /// ```sh
    /// cargo test --package JASS-Tree-sitter-Rust w3e::slk::tests::dump_unit_field_names -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_unit_field_names() {
        use std::collections::BTreeSet;

        let files: &[(&str, &[u8])] = &[
            ("UnitData.slk", include_bytes!("../../lng/slk/fixtures/Units/UnitData.slk")),
            ("UnitBalance.slk", include_bytes!("../../lng/slk/fixtures/Units/UnitBalance.slk")),
            ("unitUI.slk", include_bytes!("../../lng/slk/fixtures/Units/unitUI.slk")),
            ("UnitWeapons.slk", include_bytes!("../../lng/slk/fixtures/Units/UnitWeapons.slk")),
        ];

        let mut all_fields = BTreeSet::new();

        for (name, data) in files {
            let rows = parse_slk(data);
            assert!(!rows.is_empty(), "should parse rows from {}", name);

            let mut fields = BTreeSet::new();
            for row in &rows {
                for key in row.keys() {
                    fields.insert(key.clone());
                }
            }

            println!("\n── {} ({} fields, {} rows) ──", name, fields.len(), rows.len());
            for f in &fields {
                println!("  {}", f);
                all_fields.insert(format!("{}:{}", name, f));
            }
        }

        let out_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/lng/slk/fixtures/Units/unit_field_names.txt"
        );
        let content = all_fields.iter().cloned().collect::<Vec<_>>().join("\n");
        std::fs::write(out_path, &content).expect("failed to write unit_field_names.txt");

        println!("\nWrote {} field names to {}", all_fields.len(), out_path);
    }

    /// Dump races, types, and unit names from unit SLK files.
    ///
    /// Run manually:
    /// ```sh
    /// cargo test --package JASS-Tree-sitter-Rust w3e::slk::tests::dump_unit_races_and_types -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_unit_races_and_types() {
        use std::collections::BTreeMap;

        let data = include_bytes!("../../lng/slk/fixtures/Units/UnitData.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some unit rows");

        let bal_data = include_bytes!("../../lng/slk/fixtures/Units/UnitBalance.slk");
        let bal_rows = parse_slk(bal_data);
        let mut bal_map: BTreeMap<String, HashMap<String, String>> = BTreeMap::new();
        for row in bal_rows {
            if let Some(id) = row.get("unitBalanceID") {
                bal_map.insert(id.clone(), row);
            }
        }

        let ui_data = include_bytes!("../../lng/slk/fixtures/Units/unitUI.slk");
        let ui_rows = parse_slk(ui_data);
        let mut ui_map: BTreeMap<String, HashMap<String, String>> = BTreeMap::new();
        for row in ui_rows {
            if let Some(id) = row.get("unitUIID") {
                ui_map.insert(id.clone(), row);
            }
        }

        let mut races: BTreeMap<String, usize> = BTreeMap::new();
        let mut types: BTreeMap<String, usize> = BTreeMap::new();

        for row in &rows {
            let unit_id = row.get("unitID").cloned().unwrap_or_default();
            if unit_id.is_empty() { continue; }

            let race = row.get("race").cloned().unwrap_or_default();
            if !race.is_empty() {
                *races.entry(race).or_insert(0) += 1;
            }

            if let Some(bal) = bal_map.get(&unit_id) {
                let t = bal.get("type").cloned().unwrap_or_default();
                if !t.is_empty() {
                    *types.entry(t).or_insert(0) += 1;
                }
            }
        }

        println!("\n══════════════════════════════════════════");
        println!("  Unit SLK — {} entries", rows.len());
        println!("══════════════════════════════════════════\n");

        println!("── Races ({}) ──", races.len());
        for (r, count) in &races {
            println!("  {:<20} {:>4} units", r, count);
        }

        println!("\n── Types ({}) ──", types.len());
        for (t, count) in &types {
            println!("  {:<20} {:>4} units", t, count);
        }

        println!("\n── Unit names (first 30) ──");
        for row in rows.iter().take(30) {
            let id = row.get("unitID").cloned().unwrap_or_default();
            let name = ui_map.get(&id).and_then(|u| u.get("name")).cloned().unwrap_or_default();
            let race = row.get("race").cloned().unwrap_or_default();
            let comment = row.get("comment(s)").cloned().unwrap_or_default();
            println!("  {} | {:<35} | race={:<12} | {}", id, name, race, comment);
        }

        println!("\nTotal races: {}", races.len());
        println!("Total types: {}", types.len());
        println!("Total unit entries: {}", rows.len());
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

    #[test]
    fn parse_unit_strings_fixture() {
        let data = include_bytes!("../../lng/bni/fixtures/Units/UndeadUnitStrings.txt");
        let sections = parse_unit_strings(data);
        assert!(!sections.is_empty(), "should parse some sections");

        // Check a hero entry
        let ucrl = sections.get("Ucrl").expect("should have [Ucrl] section");
        assert_eq!(ucrl.get("Name").map(|s| s.as_str()), Some("Crypt Lord"));
        assert_eq!(ucrl.get("Hotkey").map(|s| s.as_str()), Some("C"));
        assert!(ucrl.get("Propernames").is_some());
        assert!(ucrl.get("Ubertip").is_some());
        assert!(ucrl.get("Revivetip").is_some());
        assert!(ucrl.get("Awakentip").is_some());

        // Check a regular unit entry
        let ugho = sections.get("ugho").expect("should have [ugho] section");
        assert_eq!(ugho.get("Name").map(|s| s.as_str()), Some("Ghoul"));
        assert_eq!(ugho.get("Hotkey").map(|s| s.as_str()), Some("G"));

        // Check a building entry
        let unpl = sections.get("unpl").expect("should have [unpl] section");
        assert_eq!(unpl.get("Name").map(|s| s.as_str()), Some("Necropolis"));
        assert_eq!(unpl.get("Hotkey").map(|s| s.as_str()), Some("N"));

        // Check an entry with Casterupgradename
        let uban = sections.get("uban").expect("should have [uban] section");
        assert_eq!(uban.get("Name").map(|s| s.as_str()), Some("Banshee"));
        assert!(uban.get("Casterupgradename").is_some());
        assert!(uban.get("Casterupgradetip").is_some());
    }

    #[test]
    fn parse_human_unit_strings_fixture() {
        let data = include_bytes!("../../lng/bni/fixtures/Units/HumanUnitStrings.txt");
        let sections = parse_unit_strings(data);
        assert!(!sections.is_empty(), "should parse some sections");

        let hamg = sections.get("Hamg").expect("should have [Hamg] section");
        assert_eq!(hamg.get("Name").map(|s| s.as_str()), Some("Archmage"));
        assert_eq!(hamg.get("Hotkey").map(|s| s.as_str()), Some("A"));
        assert!(hamg.get("Propernames").is_some());
    }
}

