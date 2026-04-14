//! Destructable data from `Units\DestructableData.slk`.
//!
//! war3map.w3b — Destructable data

use serde::Serialize;
use std::collections::HashMap;
use super::{Color, parse_slk, slk_u8, slk_u32, slk_f64, slk_bool, slk_str, rawcode_to_u32};
use super::{mod_value_string, mod_value_u32, mod_value_f64, mod_value_bool};
use crate::lng::map_editor::westrings::GameString;
use crate::lng::w3abdhqtu::parse::{W3ObjectData, ModificationValue};

// ─── Destructable ────────────────────────────────────────────────────────────

/// A single destructable entry extracted from `Units\DestructableData.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Destructable {
    /// Rawcode text, e.g. `"ATtr"`.
    pub destructable_id: String,
    /// Base (original) rawcode for custom destructables from `.w3b`.
    /// Empty string for standard destructables loaded from `DestructableData.slk`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub base_id: String,
    /// Whether this destructable was modified by `war3map.w3b` (original or custom).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub w3b_modified: bool,
    /// Default (pre-modification) values for fields changed by `.w3b`.
    /// Key = JS property name (e.g. `"name"`, `"hp"`), value = display string.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub defaults: HashMap<String, String>,
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
    /// Default destructables from the SLK (before `war3map.w3b` merge).
    /// This is the "base" data from the map's own SLK or the game installation.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub destructables_default: HashMap<u32, Destructable>,
    /// Destructables keyed by rawcode `u32` — the **current** (merged) state
    /// after applying `war3map.w3b` originals and customs.
    ///
    /// Only entries touched by `.w3b` have `w3b_modified = true`.
    /// Only entries created by `.w3b` customs have `base_id` set.
    pub destructables: HashMap<u32, Destructable>,
    /// Errors encountered while merging `war3map.w3b` data.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub w3b_errors: Vec<String>,
}

/// Try to load and parse `Units\DestructableData.slk` via the cascading lookup.
pub fn load_destructables_slk(archive_path: Option<&str>) -> Option<DestructablesSlkResult> {
    let (buf, source) = crate::lng::map_editor::file_lookup::lookup_file(
        "Units\\DestructableData.slk",
        archive_path,
    )?;

    // Ensure WorldEditStrings are loaded for WESTRING_* resolution.
    crate::lng::map_editor::westrings::ensure_loaded(archive_path);

    let rows = parse_slk(&buf);

    let mut destructables = HashMap::new();
    for row in rows {
        let destructable_id = match row.get("DestructableID") {
            Some(id) if !id.is_empty() => id.clone(),
            _ => continue,
        };

        // Resolve WESTRING_* references in the Name field.
        let raw_name = row.get("Name").cloned().unwrap_or_default();
        let name = crate::lng::map_editor::westrings::resolve_game_string(&raw_name);

        // Resolve WESTRING_* references in EditorSuffix.
        let raw_suffix = row.get("EditorSuffix")
            .filter(|v| *v != "_" && *v != "-")
            .cloned()
            .unwrap_or_default();
        let editor_suffix = crate::lng::map_editor::westrings::resolve_game_string(&raw_suffix);

        // Resolve WESTRING_* references in comment.
        let raw_comment = row.get("comment").cloned().unwrap_or_default();
        let comment = crate::lng::map_editor::westrings::resolve_game_string(&raw_comment);

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

        let key = rawcode_to_u32(&destructable_id);
        destructables.insert(key, Destructable {
            destructable_id,
            base_id: String::new(),
            w3b_modified: false,
            defaults: HashMap::new(),
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
            max_pitch: slk_f64(&row, "maxPitch", 0.0),
            max_roll: slk_f64(&row, "maxRoll", 0.0),
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
        destructables_default: HashMap::new(),
        destructables,
        w3b_errors: Vec::new(),
    })
}

// ─── DestructableMetaData.slk ────────────────────────────────────────────────

/// A single entry from `Units\DestructableMetaData.slk`.
/// Maps a 4-char rawcode (e.g. `"bnam"`) to a field name (e.g. `"Name"`).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DestructableMetaEntry {
    /// 4-char rawcode, e.g. `"bnam"`.
    pub id: String,
    /// Field name in `DestructableData.slk`, e.g. `"Name"`.
    pub field: String,
    /// Value type, e.g. `"string"`, `"int"`, `"unreal"`, `"bool"`.
    pub meta_type: String,
}

/// Map of 4-char rawcode → `DestructableMetaEntry`.
pub type DestructableMetaMap = HashMap<String, DestructableMetaEntry>;

/// Load and parse the embedded `Units\DestructableMetaData.slk`.
pub fn load_destructable_metadata() -> DestructableMetaMap {
    let data = include_bytes!("../../../lng/slk/fixtures/Units/DestructableMetaData.slk");
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
        map.insert(id.clone(), DestructableMetaEntry { id, field, meta_type });
    }
    map
}

// ─── Merge war3map.w3b into destructables ────────────────────────────────────

/// Apply `war3map.w3b` modifications to the destructables map.
///
/// Returns a list of human-readable error messages for problems encountered.
pub fn merge_w3b_into_destructables(
    destructables: &mut HashMap<u32, Destructable>,
    w3b: &W3ObjectData,
    meta: &DestructableMetaMap,
) -> Vec<String> {
    let mut errors = Vec::new();

    // Snapshot of the default (pre-modification) map.
    // Customs must clone from defaults, not from originals-modified entries.
    let defaults = destructables.clone();

    // Process original-table modifications (modify existing base destructables in-place).
    for def in &w3b.table.originals {
        let key = def.original_id.raw;
        let base = match destructables.get_mut(&key) {
            Some(d) => d,
            None => {
                errors.push(format!(
                    "w3b original: base destructable '{}' (0x{:08X}) not found in DestructableData.slk",
                    def.original_id.text, key
                ));
                continue;
            }
        };
        base.w3b_modified = true;
        for set in &def.sets {
            for modif in &set.modifications {
                apply_destructable_modification(base, &modif.modification_id.text, &modif.value, meta, &mut errors);
            }
        }
    }

    // Process custom-table entries (clone from *default* destructable, set baseId, apply modifications).
    for def in &w3b.table.customs {
        let orig_key = def.original_id.raw;
        let base = match defaults.get(&orig_key) {
            Some(d) => d.clone(),
            None => {
                errors.push(format!(
                    "w3b custom: base destructable '{}' (0x{:08X}) not found in DestructableData.slk for custom '{}'",
                    def.original_id.text, orig_key, def.custom_id.text
                ));
                continue;
            }
        };

        let mut custom = base;
        custom.base_id = def.original_id.text.clone();
        custom.destructable_id = def.custom_id.text.clone();
        custom.w3b_modified = true;
        custom.defaults = HashMap::new();

        for set in &def.sets {
            for modif in &set.modifications {
                apply_destructable_modification(&mut custom, &modif.modification_id.text, &modif.value, meta, &mut errors);
            }
        }

        let custom_key = def.custom_id.raw;
        destructables.insert(custom_key, custom);
    }

    errors
}

/// Apply a single modification to a `Destructable`, using the metadata map to resolve
/// the rawcode to a field name. Saves the old value in `destructable.defaults`.
fn apply_destructable_modification(
    dest: &mut Destructable,
    mod_id: &str,
    value: &ModificationValue,
    meta: &DestructableMetaMap,
    errors: &mut Vec<String>,
) {
    let entry = match meta.get(mod_id) {
        Some(e) => e,
        None => {
            errors.push(format!(
                "w3b: unknown modification rawcode '{}' for destructable '{}'",
                mod_id, dest.destructable_id
            ));
            return;
        }
    };

    let field = entry.field.as_str();
    match field {
        "Name" => {
            if let Some(s) = mod_value_string(value) {
                dest.defaults.entry("name".into()).or_insert_with(|| dest.name.value.clone());
                dest.name = crate::lng::map_editor::westrings::resolve_game_string(&s);
            }
        }
        "EditorSuffix" => {
            if let Some(s) = mod_value_string(value) {
                dest.defaults.entry("editorSuffix".into()).or_insert_with(|| dest.editor_suffix.value.clone());
                dest.editor_suffix = crate::lng::map_editor::westrings::resolve_game_string(&s);
            }
        }
        "category" => {
            if let Some(s) = mod_value_string(value) {
                dest.defaults.entry("category".into()).or_insert_with(|| dest.category.clone());
                dest.category = s;
            }
        }
        "tilesets" => {
            if let Some(s) = mod_value_string(value) {
                dest.defaults.entry("tilesets".into()).or_insert_with(|| dest.tilesets.clone());
                dest.tilesets = s;
            }
        }
        "tilesetSpecific" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("tilesetSpecific".into()).or_insert_with(|| dest.tileset_specific.to_string());
                dest.tileset_specific = b;
            }
        }
        "file" => {
            if let Some(s) = mod_value_string(value) {
                dest.defaults.entry("file".into()).or_insert_with(|| dest.file.clone());
                dest.file = s;
            }
        }
        "lightweight" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("lightweight".into()).or_insert_with(|| dest.lightweight.to_string());
                dest.lightweight = b;
            }
        }
        "fatLOS" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("fatLos".into()).or_insert_with(|| dest.fat_los.to_string());
                dest.fat_los = b;
            }
        }
        "texID" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("texId".into()).or_insert_with(|| dest.tex_id.to_string());
                dest.tex_id = v;
            }
        }
        "texFile" => {
            if let Some(s) = mod_value_string(value) {
                dest.defaults.entry("texFile".into()).or_insert_with(|| dest.tex_file.clone());
                dest.tex_file = s;
            }
        }
        "useClickHelper" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("useClickHelper".into()).or_insert_with(|| dest.use_click_helper.to_string());
                dest.use_click_helper = b;
            }
        }
        "onCliffs" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("onCliffs".into()).or_insert_with(|| dest.on_cliffs.to_string());
                dest.on_cliffs = b;
            }
        }
        "onWater" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("onWater".into()).or_insert_with(|| dest.on_water.to_string());
                dest.on_water = b;
            }
        }
        "canPlaceDead" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("canPlaceDead".into()).or_insert_with(|| dest.can_place_dead.to_string());
                dest.can_place_dead = b;
            }
        }
        "walkable" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("walkable".into()).or_insert_with(|| dest.walkable.to_string());
                dest.walkable = b;
            }
        }
        "cliffHeight" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("cliffHeight".into()).or_insert_with(|| dest.cliff_height.to_string());
                dest.cliff_height = v;
            }
        }
        "targType" => {
            if let Some(s) = mod_value_string(value) {
                dest.defaults.entry("targType".into()).or_insert_with(|| dest.targ_type.clone());
                dest.targ_type = s;
            }
        }
        "armor" => {
            if let Some(s) = mod_value_string(value) {
                dest.defaults.entry("armor".into()).or_insert_with(|| dest.armor.clone());
                dest.armor = s;
            }
        }
        "numVar" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("numVar".into()).or_insert_with(|| dest.num_var.to_string());
                dest.num_var = v;
            }
        }
        "HP" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("hp".into()).or_insert_with(|| dest.hp.to_string());
                dest.hp = v;
            }
        }
        "occH" => {
            if let Some(v) = mod_value_f64(value) {
                dest.defaults.entry("occH".into()).or_insert_with(|| dest.occ_h.to_string());
                dest.occ_h = v;
            }
        }
        "flyH" => {
            if let Some(v) = mod_value_f64(value) {
                dest.defaults.entry("flyH".into()).or_insert_with(|| dest.fly_h.to_string());
                dest.fly_h = v;
            }
        }
        "fixedRot" => {
            if let Some(v) = mod_value_f64(value) {
                dest.defaults.entry("fixedRot".into()).or_insert_with(|| dest.fixed_rot.to_string());
                dest.fixed_rot = v;
            }
        }
        "selSize" => {
            if let Some(v) = mod_value_f64(value) {
                dest.defaults.entry("selSize".into()).or_insert_with(|| dest.sel_size.to_string());
                dest.sel_size = v;
            }
        }
        "minScale" => {
            if let Some(v) = mod_value_f64(value) {
                dest.defaults.entry("minScale".into()).or_insert_with(|| dest.min_scale.to_string());
                dest.min_scale = v;
            }
        }
        "maxScale" => {
            if let Some(v) = mod_value_f64(value) {
                dest.defaults.entry("maxScale".into()).or_insert_with(|| dest.max_scale.to_string());
                dest.max_scale = v;
            }
        }
        "canPlaceRandScale" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("canPlaceRandScale".into()).or_insert_with(|| dest.can_place_rand_scale.to_string());
                dest.can_place_rand_scale = b;
            }
        }
        "maxPitch" => {
            if let Some(v) = mod_value_f64(value) {
                dest.defaults.entry("maxPitch".into()).or_insert_with(|| dest.max_pitch.to_string());
                dest.max_pitch = v;
            }
        }
        "maxRoll" => {
            if let Some(v) = mod_value_f64(value) {
                dest.defaults.entry("maxRoll".into()).or_insert_with(|| dest.max_roll.to_string());
                dest.max_roll = v;
            }
        }
        "radius" => {
            if let Some(v) = mod_value_f64(value) {
                dest.defaults.entry("radius".into()).or_insert_with(|| dest.radius.to_string());
                dest.radius = v;
            }
        }
        "fogRadius" => {
            if let Some(v) = mod_value_f64(value) {
                dest.defaults.entry("fogRadius".into()).or_insert_with(|| dest.fog_radius.to_string());
                dest.fog_radius = v;
            }
        }
        "fogVis" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("fogVis".into()).or_insert_with(|| dest.fog_vis.to_string());
                dest.fog_vis = b;
            }
        }
        "pathTex" => {
            if let Some(s) = mod_value_string(value) {
                dest.defaults.entry("pathTex".into()).or_insert_with(|| dest.path_tex.clone());
                dest.path_tex = s;
            }
        }
        "pathTexDeath" => {
            if let Some(s) = mod_value_string(value) {
                dest.defaults.entry("pathTexDeath".into()).or_insert_with(|| dest.path_tex_death.clone());
                dest.path_tex_death = s;
            }
        }
        "deathSnd" => {
            if let Some(s) = mod_value_string(value) {
                dest.defaults.entry("deathSnd".into()).or_insert_with(|| dest.death_snd.clone());
                dest.death_snd = s;
            }
        }
        "shadow" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("shadow".into()).or_insert_with(|| dest.shadow.to_string());
                dest.shadow = b;
            }
        }
        "showInMM" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("showInMm".into()).or_insert_with(|| dest.show_in_mm.to_string());
                dest.show_in_mm = b;
            }
        }
        "useMMColor" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("useMmColor".into()).or_insert_with(|| dest.use_mm_color.to_string());
                dest.use_mm_color = b;
            }
        }
        "MMRed" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("mmColor".into()).or_insert_with(|| {
                    format!("{},{},{}", dest.mm_color.r, dest.mm_color.g, dest.mm_color.b)
                });
                dest.mm_color.r = v.min(255) as u8;
            }
        }
        "MMGreen" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("mmColor".into()).or_insert_with(|| {
                    format!("{},{},{}", dest.mm_color.r, dest.mm_color.g, dest.mm_color.b)
                });
                dest.mm_color.g = v.min(255) as u8;
            }
        }
        "MMBlue" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("mmColor".into()).or_insert_with(|| {
                    format!("{},{},{}", dest.mm_color.r, dest.mm_color.g, dest.mm_color.b)
                });
                dest.mm_color.b = v.min(255) as u8;
            }
        }
        "colorR" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("color".into()).or_insert_with(|| {
                    format!("{},{},{}", dest.color.r, dest.color.g, dest.color.b)
                });
                dest.color.r = v.min(255) as u8;
            }
        }
        "colorG" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("color".into()).or_insert_with(|| {
                    format!("{},{},{}", dest.color.r, dest.color.g, dest.color.b)
                });
                dest.color.g = v.min(255) as u8;
            }
        }
        "colorB" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("color".into()).or_insert_with(|| {
                    format!("{},{},{}", dest.color.r, dest.color.g, dest.color.b)
                });
                dest.color.b = v.min(255) as u8;
            }
        }
        "buildTime" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("buildTime".into()).or_insert_with(|| dest.build_time.to_string());
                dest.build_time = v;
            }
        }
        "repairTime" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("repairTime".into()).or_insert_with(|| dest.repair_time.to_string());
                dest.repair_time = v;
            }
        }
        "goldRep" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("goldRep".into()).or_insert_with(|| dest.gold_rep.to_string());
                dest.gold_rep = v;
            }
        }
        "lumberRep" => {
            if let Some(v) = mod_value_u32(value) {
                dest.defaults.entry("lumberRep".into()).or_insert_with(|| dest.lumber_rep.to_string());
                dest.lumber_rep = v;
            }
        }
        "selectable" => {
            if let Some(b) = mod_value_bool(value) {
                dest.defaults.entry("selectable".into()).or_insert_with(|| dest.selectable.to_string());
                dest.selectable = b;
            }
        }
        "selcircsize" => {
            if let Some(v) = mod_value_f64(value) {
                dest.defaults.entry("selcircsize".into()).or_insert_with(|| dest.selcircsize.to_string());
                dest.selcircsize = v;
            }
        }
        "portraitmodel" => {
            if let Some(s) = mod_value_string(value) {
                dest.defaults.entry("portraitmodel".into()).or_insert_with(|| dest.portraitmodel.clone());
                dest.portraitmodel = s;
            }
        }
        "doodClass" => {
            if let Some(s) = mod_value_string(value) {
                dest.defaults.entry("doodClass".into()).or_insert_with(|| dest.dood_class.clone());
                dest.dood_class = s;
            }
        }
        "UserList" => {} // editor-only field, ignore
        _ => {
            // Unknown field — not critical, just log
        }
    }
}
