//! Destructable data from `Units\DestructableData.slk`.
//!
//! war3map.w3b — Destructable data

use serde::Serialize;
use std::collections::HashMap;
use super::{Color, parse_slk, slk_u8, slk_u32, slk_f64, slk_bool, slk_str, rawcode_to_u32};
use crate::lng::w3e::westrings::GameString;

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

/// Try to load and parse `Units\DestructableData.slk` via the cascading lookup.
pub fn load_destructables_slk(archive_path: Option<&str>) -> Option<DestructablesSlkResult> {
    let (buf, source) = crate::lng::w3e::file_lookup::lookup_file(
        "Units\\DestructableData.slk",
        archive_path,
    )?;

    // Ensure WorldEditStrings are loaded for WESTRING_* resolution.
    crate::lng::w3e::westrings::ensure_loaded(archive_path);

    let rows = parse_slk(&buf);

    let mut destructables = HashMap::new();
    for row in rows {
        let destructable_id = match row.get("DestructableID") {
            Some(id) if !id.is_empty() => id.clone(),
            _ => continue,
        };

        // Resolve WESTRING_* references in the Name field.
        let raw_name = row.get("Name").cloned().unwrap_or_default();
        let name = crate::lng::w3e::westrings::resolve_game_string(&raw_name);

        // Resolve WESTRING_* references in EditorSuffix.
        let raw_suffix = row.get("EditorSuffix")
            .filter(|v| *v != "_" && *v != "-")
            .cloned()
            .unwrap_or_default();
        let editor_suffix = crate::lng::w3e::westrings::resolve_game_string(&raw_suffix);

        // Resolve WESTRING_* references in comment.
        let raw_comment = row.get("comment").cloned().unwrap_or_default();
        let comment = crate::lng::w3e::westrings::resolve_game_string(&raw_comment);

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
    fn parse_destructable_slk_fixture() {
        let data = include_bytes!("../../../lng/slk/fixtures/Units/DestructableData.slk");
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
    /// cargo test --package JASS-Tree-sitter-Rust w3e::slk::destructable::tests::dump_destructable_categories_and_tilesets -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_destructable_categories_and_tilesets() {
        use std::collections::BTreeMap;

        let data = include_bytes!("../../../lng/slk/fixtures/Units/DestructableData.slk");
        let rows = parse_slk(data);
        assert!(!rows.is_empty(), "should parse some destructable rows");

        let mut categories: BTreeMap<String, usize> = BTreeMap::new();
        let mut tilesets: BTreeMap<char, usize> = BTreeMap::new();
        let mut names: Vec<(String, String, String, String)> = Vec::new();

        for row in &rows {
            let dest_id = row.get("DestructableID").cloned().unwrap_or_default();
            if dest_id.is_empty() {
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
    /// cargo test --package JASS-Tree-sitter-Rust w3e::slk::destructable::tests::dump_destructable_field_names -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn dump_destructable_field_names() {
        use std::collections::BTreeSet;

        let data = include_bytes!("../../../lng/slk/fixtures/Units/DestructableData.slk");
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

