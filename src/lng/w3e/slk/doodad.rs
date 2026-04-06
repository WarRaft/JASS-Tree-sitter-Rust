//! Doodad data from `Doodads\Doodads.slk` + `war3map.w3d` merge.
//!
//! war3map.w3d — Doodad data

use serde::Serialize;
use std::collections::HashMap;
use super::{Color, parse_slk, slk_u8, slk_u32, slk_f64, slk_bool, slk_str, rawcode_to_u32};
use super::{mod_value_string, mod_value_u32, mod_value_f64, mod_value_bool};
use crate::lng::w3e::westrings::GameString;
use crate::lng::w3abdhqtu::parse::{W3ObjectData, ModificationValue};

// ─── Doodad ──────────────────────────────────────────────────────────────────

/// A single doodad entry extracted from `Doodads\Doodads.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Doodad {
    /// Rawcode text, e.g. `"APms"`.
    pub dood_id: String,
    /// Base (original) rawcode for custom doodads from `.w3d`.
    /// Empty string for standard doodads loaded from `Doodads.slk`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub base_id: String,
    /// Whether this doodad was modified by `war3map.w3d` (original or custom).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub w3d_modified: bool,
    /// Default (pre-modification) values for fields changed by `.w3d`.
    /// Key = JS property name (e.g. `"name"`, `"defScale"`), value = display string.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub defaults: HashMap<String, String>,
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
    /// Errors encountered while merging `war3map.w3d` data.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub w3d_errors: Vec<String>,
}

/// Try to load and parse `Doodads\Doodads.slk` via the cascading lookup.
pub fn load_doodads_slk(archive_path: Option<&str>) -> Option<DoodadsSlkResult> {
    let (buf, source) = crate::lng::w3e::file_lookup::lookup_file(
        "Doodads\\Doodads.slk",
        archive_path,
    )?;

    // Ensure WorldEditStrings are loaded for WESTRING_* resolution.
    crate::lng::w3e::westrings::ensure_loaded(archive_path);

    let rows = parse_slk(&buf);

    let mut doodads = HashMap::new();
    for row in rows {
        let dood_id = match row.get("doodID") {
            Some(id) if !id.is_empty() => id.clone(),
            _ => continue,
        };

        // Resolve WESTRING_* references in the Name field.
        let raw_name = row.get("Name").cloned().unwrap_or_default();
        let name = crate::lng::w3e::westrings::resolve_game_string(&raw_name);

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

        let key = rawcode_to_u32(&dood_id);
        doodads.insert(key, Doodad {
            dood_id,
            base_id: String::new(),
            w3d_modified: false,
            defaults: HashMap::new(),
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
        w3d_errors: Vec::new(),
    })
}

// ─── DoodadMetaData.slk ──────────────────────────────────────────────────────

/// A single entry from `Doodads\DoodadMetaData.slk`.
/// Maps a 4-char rawcode (e.g. `"dnam"`) to a field name (e.g. `"Name"`).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DoodadMetaEntry {
    /// 4-char rawcode, e.g. `"dnam"`.
    pub id: String,
    /// Field name in `Doodads.slk`, e.g. `"Name"`.
    pub field: String,
    /// Value type, e.g. `"string"`, `"int"`, `"unreal"`, `"bool"`.
    pub meta_type: String,
}

/// Map of 4-char rawcode → `DoodadMetaEntry`.
pub type DoodadMetaMap = HashMap<String, DoodadMetaEntry>;

/// Load and parse the embedded `Doodads\DoodadMetaData.slk`.
pub fn load_doodad_metadata() -> DoodadMetaMap {
    let data = include_bytes!("../../../lng/slk/fixtures/Doodads/DoodadMetaData.slk");
    let rows = parse_slk(data);

    let mut map = HashMap::new();
    for row in rows {
        let id = match row.get("ID") {
            Some(s) if !s.is_empty() => s.clone(),
            _ => continue,
        };
        let field = row.get("field").cloned().unwrap_or_default();
        let meta_type = row.get("type").cloned().unwrap_or_default();
        if field.is_empty() { continue; }
        map.insert(id.clone(), DoodadMetaEntry { id, field, meta_type });
    }
    map
}

// ─── Merge war3map.w3d into doodads ──────────────────────────────────────────

/// Apply `war3map.w3d` modifications to the doodads map.
///
/// Returns a list of human-readable error messages for problems encountered.
pub fn merge_w3d_into_doodads(
    doodads: &mut HashMap<u32, Doodad>,
    w3d: &W3ObjectData,
    meta: &DoodadMetaMap,
) -> Vec<String> {
    let mut errors = Vec::new();

    // Process original-table modifications (modify existing base doodads in-place).
    for def in &w3d.table.originals {
        let key = def.original_id.raw;
        let base = match doodads.get_mut(&key) {
            Some(d) => d,
            None => {
                errors.push(format!(
                    "w3d original: base doodad '{}' (0x{:08X}) not found in Doodads.slk",
                    def.original_id.text, key
                ));
                continue;
            }
        };
        base.w3d_modified = true;
        for set in &def.sets {
            for modif in &set.modifications {
                apply_doodad_modification(base, &modif.modification_id.text, &modif.value, meta, &mut errors);
            }
        }
    }

    // Process custom-table entries (clone base doodad, set baseId, apply modifications).
    for def in &w3d.table.customs {
        let orig_key = def.original_id.raw;
        let base = match doodads.get(&orig_key) {
            Some(d) => d.clone(),
            None => {
                errors.push(format!(
                    "w3d custom: base doodad '{}' (0x{:08X}) not found in Doodads.slk for custom '{}'",
                    def.original_id.text, orig_key, def.custom_id.text
                ));
                continue;
            }
        };

        let mut custom = base;
        custom.base_id = def.original_id.text.clone();
        custom.dood_id = def.custom_id.text.clone();
        custom.w3d_modified = true;
        custom.defaults = HashMap::new();

        for set in &def.sets {
            for modif in &set.modifications {
                apply_doodad_modification(&mut custom, &modif.modification_id.text, &modif.value, meta, &mut errors);
            }
        }

        let custom_key = def.custom_id.raw;
        doodads.insert(custom_key, custom);
    }

    errors
}

/// Apply a single modification to a `Doodad`, using the metadata map to resolve
/// the rawcode to a field name. Saves the old value in `doodad.defaults`.
fn apply_doodad_modification(
    doodad: &mut Doodad,
    mod_id: &str,
    value: &ModificationValue,
    meta: &DoodadMetaMap,
    errors: &mut Vec<String>,
) {
    let entry = match meta.get(mod_id) {
        Some(e) => e,
        None => {
            errors.push(format!(
                "w3d: unknown modification rawcode '{}' for doodad '{}'",
                mod_id, doodad.dood_id
            ));
            return;
        }
    };

    let field = entry.field.as_str();
    match field {
        "Name" => {
            if let Some(s) = mod_value_string(value) {
                doodad.defaults.entry("name".into()).or_insert_with(|| doodad.name.value.clone());
                doodad.name = crate::lng::w3e::westrings::resolve_game_string(&s);
            }
        }
        "category" => {
            if let Some(s) = mod_value_string(value) {
                doodad.defaults.entry("category".into()).or_insert_with(|| doodad.category.clone());
                doodad.category = s;
            }
        }
        "tilesets" => {
            if let Some(s) = mod_value_string(value) {
                doodad.defaults.entry("tilesets".into()).or_insert_with(|| doodad.tilesets.clone());
                doodad.tilesets = s;
            }
        }
        "tilesetSpecific" => {
            if let Some(b) = mod_value_bool(value) {
                doodad.defaults.entry("tilesetSpecific".into()).or_insert_with(|| doodad.tileset_specific.to_string());
                doodad.tileset_specific = b;
            }
        }
        "file" => {
            if let Some(s) = mod_value_string(value) {
                doodad.defaults.entry("file".into()).or_insert_with(|| doodad.file.clone());
                doodad.file = s;
            }
        }
        "doodClass" => {
            if let Some(s) = mod_value_string(value) {
                doodad.defaults.entry("doodClass".into()).or_insert_with(|| doodad.dood_class.clone());
                doodad.dood_class = s;
            }
        }
        "soundLoop" => {
            if let Some(s) = mod_value_string(value) {
                doodad.defaults.entry("soundLoop".into()).or_insert_with(|| doodad.sound_loop.clone());
                doodad.sound_loop = s;
            }
        }
        "numVar" => {
            if let Some(v) = mod_value_u32(value) {
                doodad.defaults.entry("numVar".into()).or_insert_with(|| doodad.num_var.to_string());
                doodad.num_var = v;
            }
        }
        "defScale" => {
            if let Some(v) = mod_value_f64(value) {
                doodad.defaults.entry("defScale".into()).or_insert_with(|| doodad.def_scale.to_string());
                doodad.def_scale = v;
            }
        }
        "minScale" => {
            if let Some(v) = mod_value_f64(value) {
                doodad.defaults.entry("minScale".into()).or_insert_with(|| doodad.min_scale.to_string());
                doodad.min_scale = v;
            }
        }
        "maxScale" => {
            if let Some(v) = mod_value_f64(value) {
                doodad.defaults.entry("maxScale".into()).or_insert_with(|| doodad.max_scale.to_string());
                doodad.max_scale = v;
            }
        }
        "canPlaceRandScale" => {
            if let Some(b) = mod_value_bool(value) {
                doodad.defaults.entry("canPlaceRandScale".into()).or_insert_with(|| doodad.can_place_rand_scale.to_string());
                doodad.can_place_rand_scale = b;
            }
        }
        "selSize" => {
            if let Some(v) = mod_value_f64(value) {
                doodad.defaults.entry("selSize".into()).or_insert_with(|| doodad.sel_size.to_string());
                doodad.sel_size = v;
            }
        }
        "useClickHelper" => {
            if let Some(b) = mod_value_bool(value) {
                doodad.defaults.entry("useClickHelper".into()).or_insert_with(|| doodad.use_click_helper.to_string());
                doodad.use_click_helper = b;
            }
        }
        "ignoreModelClick" => {
            if let Some(b) = mod_value_bool(value) {
                doodad.defaults.entry("ignoreModelClick".into()).or_insert_with(|| doodad.ignore_model_click.to_string());
                doodad.ignore_model_click = b;
            }
        }
        "maxPitch" => {
            if let Some(v) = mod_value_f64(value) {
                doodad.defaults.entry("maxPitch".into()).or_insert_with(|| doodad.max_pitch.to_string());
                doodad.max_pitch = v;
            }
        }
        "maxRoll" => {
            if let Some(v) = mod_value_f64(value) {
                doodad.defaults.entry("maxRoll".into()).or_insert_with(|| doodad.max_roll.to_string());
                doodad.max_roll = v;
            }
        }
        "visRadius" => {
            if let Some(v) = mod_value_f64(value) {
                doodad.defaults.entry("visRadius".into()).or_insert_with(|| doodad.vis_radius.to_string());
                doodad.vis_radius = v;
            }
        }
        "walkable" => {
            if let Some(b) = mod_value_bool(value) {
                doodad.defaults.entry("walkable".into()).or_insert_with(|| doodad.walkable.to_string());
                doodad.walkable = b;
            }
        }
        "onCliffs" => {
            if let Some(b) = mod_value_bool(value) {
                doodad.defaults.entry("onCliffs".into()).or_insert_with(|| doodad.on_cliffs.to_string());
                doodad.on_cliffs = b;
            }
        }
        "onWater" => {
            if let Some(b) = mod_value_bool(value) {
                doodad.defaults.entry("onWater".into()).or_insert_with(|| doodad.on_water.to_string());
                doodad.on_water = b;
            }
        }
        "floats" => {
            if let Some(b) = mod_value_bool(value) {
                doodad.defaults.entry("floats".into()).or_insert_with(|| doodad.floats.to_string());
                doodad.floats = b;
            }
        }
        "shadow" => {
            if let Some(b) = mod_value_bool(value) {
                doodad.defaults.entry("shadow".into()).or_insert_with(|| doodad.shadow.to_string());
                doodad.shadow = b;
            }
        }
        "showInFog" => {
            if let Some(b) = mod_value_bool(value) {
                doodad.defaults.entry("showInFog".into()).or_insert_with(|| doodad.show_in_fog.to_string());
                doodad.show_in_fog = b;
            }
        }
        "animInFog" => {
            if let Some(b) = mod_value_bool(value) {
                doodad.defaults.entry("animInFog".into()).or_insert_with(|| doodad.anim_in_fog.to_string());
                doodad.anim_in_fog = b;
            }
        }
        "fixedRot" => {
            if let Some(v) = mod_value_f64(value) {
                doodad.defaults.entry("fixedRot".into()).or_insert_with(|| doodad.fixed_rot.to_string());
                doodad.fixed_rot = v;
            }
        }
        "pathTex" => {
            if let Some(s) = mod_value_string(value) {
                doodad.defaults.entry("pathTex".into()).or_insert_with(|| doodad.path_tex.clone());
                doodad.path_tex = s;
            }
        }
        "showInMM" => {
            if let Some(b) = mod_value_bool(value) {
                doodad.defaults.entry("showInMm".into()).or_insert_with(|| doodad.show_in_mm.to_string());
                doodad.show_in_mm = b;
            }
        }
        "useMMColor" => {
            if let Some(b) = mod_value_bool(value) {
                doodad.defaults.entry("useMmColor".into()).or_insert_with(|| doodad.use_mm_color.to_string());
                doodad.use_mm_color = b;
            }
        }
        "MMRed" => {
            if let Some(v) = mod_value_u32(value) {
                doodad.defaults.entry("mmColor".into()).or_insert_with(|| {
                    format!("{},{},{}", doodad.mm_color.r, doodad.mm_color.g, doodad.mm_color.b)
                });
                doodad.mm_color.r = v.min(255) as u8;
            }
        }
        "MMGreen" => {
            if let Some(v) = mod_value_u32(value) {
                doodad.defaults.entry("mmColor".into()).or_insert_with(|| {
                    format!("{},{},{}", doodad.mm_color.r, doodad.mm_color.g, doodad.mm_color.b)
                });
                doodad.mm_color.g = v.min(255) as u8;
            }
        }
        "MMBlue" => {
            if let Some(v) = mod_value_u32(value) {
                doodad.defaults.entry("mmColor".into()).or_insert_with(|| {
                    format!("{},{},{}", doodad.mm_color.r, doodad.mm_color.g, doodad.mm_color.b)
                });
                doodad.mm_color.b = v.min(255) as u8;
            }
        }
        // Vertex colour fields (vertR, vertG, vertB) with variations — skip for now
        "vertR" | "vertG" | "vertB" => {}
        "UserList" => {} // editor-only field, ignore
        _ => {
            // Unknown field — not critical, just log
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_doodads_slk_fixture() {
        let data = include_bytes!("../../../lng/slk/fixtures/Doodads/Doodads.slk");
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
    /// cargo test --package JASS-Tree-sitter-Rust w3e::slk::doodad::tests::dump_doodad_categories_and_tilesets -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_doodad_categories_and_tilesets() {
        use std::collections::BTreeMap;

        let data = include_bytes!("../../../lng/slk/fixtures/Doodads/Doodads.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some doodad rows");

        let mut categories: BTreeMap<String, usize> = BTreeMap::new();
        let mut tilesets: BTreeMap<char, usize> = BTreeMap::new();
        let mut names: Vec<(String, String, String, String)> = Vec::new();

        for row in &rows {
            let dood_id = row.get("doodID").cloned().unwrap_or_default();
            if dood_id.is_empty() {
                continue;
            }

            let cat = row.get("category").cloned().unwrap_or_default();
            if !cat.is_empty() {
                *categories.entry(cat.clone()).or_insert(0) += 1;
            }

            let ts = row.get("tilesets").cloned().unwrap_or_default();
            for ch in ts.chars() {
                *tilesets.entry(ch).or_insert(0) += 1;
            }

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
    /// cargo test --package JASS-Tree-sitter-Rust w3e::slk::doodad::tests::dump_doodad_field_names -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_doodad_field_names() {
        use std::collections::BTreeSet;

        let data = include_bytes!("../../../lng/slk/fixtures/Doodads/Doodads.slk");
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
}

