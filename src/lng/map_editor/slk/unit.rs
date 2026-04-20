//! Unit data from `Units\UnitData.slk` (+ Balance, UI, Weapons) + UnitStrings.
//!
//! war3map.w3u — Unit data (in W3M maps only .w3u is used, storing both unit and item data)

use super::{
    SlkSource, parse_slk, parse_unit_strings, rawcode_to_u32, slk_bool, slk_f64, slk_index_by,
    slk_str, slk_u8, slk_u32,
};
use super::{mod_value_string, mod_value_u32, mod_value_f64, mod_value_bool};
use crate::lng::map_editor::westrings::GameString;
use crate::lng::w3abdhqtu::parse::{W3ObjectData, ModificationValue};
use serde::Serialize;
use std::collections::HashMap;

/// Parsed string data from `*UnitStrings.txt` (BNI format) for a single unit.
///
/// Fields correspond to keys found in `HumanUnitStrings.txt`,
/// `OrcUnitStrings.txt`, etc.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitStringsEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ubertip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub propernames: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revivetip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub awakentip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub casterupgradename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub casterupgradetip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub casterupgradecount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_or: Option<String>,
}

impl UnitStringsEntry {
    /// Build from a raw key→value map produced by [`parse_unit_strings`].
    pub fn from_fields(fields: &HashMap<String, String>) -> Self {
        fn opt(fields: &HashMap<String, String>, key: &str) -> Option<String> {
            fields.get(key).filter(|v| !v.is_empty()).cloned()
        }
        Self {
            name: opt(fields, "Name"),
            editor_suffix: opt(fields, "EditorSuffix"),
            tip: opt(fields, "Tip"),
            // Accept both "Ubertip" and "UberTip"
            ubertip: opt(fields, "Ubertip").or_else(|| opt(fields, "UberTip")),
            tooltip: opt(fields, "Tooltip"),
            hotkey: opt(fields, "Hotkey"),
            propernames: opt(fields, "Propernames"),
            revivetip: opt(fields, "Revivetip"),
            awakentip: opt(fields, "Awakentip"),
            description: opt(fields, "Description"),
            casterupgradename: opt(fields, "Casterupgradename"),
            casterupgradetip: opt(fields, "Casterupgradetip"),
            casterupgradecount: opt(fields, "Casterupgradecount"),
            dependency_or: opt(fields, "DependencyOr"),
        }
    }

    /// Merge another entry on top: only fills fields that are still `None`.
    pub fn merge(&mut self, other: &UnitStringsEntry) {
        macro_rules! fill {
            ($f:ident) => {
                if self.$f.is_none() {
                    self.$f = other.$f.clone();
                }
            };
        }
        fill!(name);
        fill!(editor_suffix);
        fill!(tip);
        fill!(ubertip);
        fill!(tooltip);
        fill!(hotkey);
        fill!(propernames);
        fill!(revivetip);
        fill!(awakentip);
        fill!(description);
        fill!(casterupgradename);
        fill!(casterupgradetip);
        fill!(casterupgradecount);
        fill!(dependency_or);
    }
}

// ─── UnitInfo ────────────────────────────────────────────────────────────────

/// A single unit entry merged from `Units\UnitData.slk`, `Units\UnitBalance.slk`,
/// `Units\unitUI.slk`, and `Units\UnitWeapons.slk`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitInfo {
    // ── Identity (UnitData) ──────────────────────────────────────
    /// Rawcode text, e.g. `"Hamg"`.
    pub unit_id: String,
    /// Base (original) rawcode for custom units from `.w3u`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub base_id: String,
    /// Whether this unit was modified by `war3map.w3u`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub w3u_modified: bool,
    /// Default (pre-modification) values for fields changed by `.w3u`.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub defaults: HashMap<String, String>,
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

/// Result of loading unit SLK files.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitsSlkResult {
    /// Where the primary file was found.
    pub source: String,
    /// Per-file source info for all loaded SLK files.
    pub sources: Vec<SlkSource>,
    /// Default units from the SLK (before `war3map.w3u` merge).
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub units_default: HashMap<u32, UnitInfo>,
    /// Units keyed by rawcode `u32` — the **current** (merged) state.
    pub units: HashMap<u32, UnitInfo>,
    /// Pre-parsed Profile strings from `*UnitStrings.txt`, keyed by rawcode.
    pub unit_strings: HashMap<u32, UnitStringsEntry>,
    /// Errors encountered while merging `war3map.w3u` data.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub w3u_errors: Vec<String>,
}

/// Try to load and parse unit SLK files via the cascading lookup.
///
/// Merges data from `UnitData.slk` (primary), `UnitBalance.slk`, `unitUI.slk`,
/// and `UnitWeapons.slk`.
pub fn load_units_slk(archive_path: Option<&str>) -> Option<UnitsSlkResult> {
    let (buf, source) =
        crate::lng::map_editor::file_lookup::lookup_file("Units\\UnitData.slk", archive_path)?;

    // Ensure WorldEditStrings are loaded for WESTRING_* resolution.
    crate::lng::map_editor::westrings::ensure_loaded(archive_path);

    let data_rows = parse_slk(&buf);

    let mut sources = vec![SlkSource {
        name: "UnitData.slk".into(),
        source: source.to_string(),
        rows: data_rows.len(),
    }];

    // Load supplementary SLK files, indexed by their ID columns.
    let balance_map =
        crate::lng::map_editor::file_lookup::lookup_file("Units\\UnitBalance.slk", archive_path)
            .map(|(b, src)| {
                let rows = parse_slk(&b);
                let n = rows.len();
                let indexed = slk_index_by(rows, "unitBalanceID");
                sources.push(SlkSource {
                    name: "UnitBalance.slk".into(),
                    source: src,
                    rows: n,
                });
                indexed
            })
            .unwrap_or_default();

    let ui_map =
        crate::lng::map_editor::file_lookup::lookup_file("Units\\unitUI.slk", archive_path)
            .map(|(b, src)| {
                let rows = parse_slk(&b);
                let n = rows.len();
                let indexed = slk_index_by(rows, "unitUIID");
                sources.push(SlkSource {
                    name: "unitUI.slk".into(),
                    source: src,
                    rows: n,
                });
                indexed
            })
            .unwrap_or_default();

    let weap_map =
        crate::lng::map_editor::file_lookup::lookup_file("Units\\UnitWeapons.slk", archive_path)
            .map(|(b, src)| {
                let rows = parse_slk(&b);
                let n = rows.len();
                let indexed = slk_index_by(rows, "unitWeapID");
                sources.push(SlkSource {
                    name: "UnitWeapons.slk".into(),
                    source: src,
                    rows: n,
                });
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
        let name = crate::lng::map_editor::westrings::resolve_game_string(&raw_name);

        let key = rawcode_to_u32(&unit_id);
        units.insert(
            key,
            UnitInfo {
                unit_id,
                base_id: String::new(),
                w3u_modified: false,
                defaults: HashMap::new(),
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
                team_color: ui
                    .get("teamColor")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(-1),
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
            },
        );
    }

    // ── Load UnitStrings.txt files ────────────────────────────────
    const UNIT_STRING_FILES: &[&str] = &[
        "Units\\HumanUnitStrings.txt",
        "Units\\OrcUnitStrings.txt",
        "Units\\NightElfUnitStrings.txt",
        "Units\\UndeadUnitStrings.txt",
        "Units\\NeutralUnitStrings.txt",
    ];

    let mut unit_strings: HashMap<u32, UnitStringsEntry> = HashMap::new();

    for &file_path in UNIT_STRING_FILES {
        if let Some((buf, src)) =
            crate::lng::map_editor::file_lookup::lookup_file(file_path, archive_path)
        {
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
                let key = rawcode_to_u32(&rawcode);
                let entry = UnitStringsEntry::from_fields(&fields);
                unit_strings
                    .entry(key)
                    .and_modify(|existing| existing.merge(&entry))
                    .or_insert(entry);
            }
        }
    }

    // Apply parsed strings to UnitInfo structs.
    for (&key, strings) in &unit_strings {
        if let Some(unit) = units.get_mut(&key) {
            if let Some(ref name_val) = strings.name {
                unit.name = crate::lng::map_editor::westrings::resolve_game_string(name_val);
            }
            if unit.tip.is_none() {
                unit.tip = strings.tip.clone();
            }
            if unit.ubertip.is_none() {
                unit.ubertip = strings.ubertip.clone();
            }
            if unit.hotkey.is_none() {
                unit.hotkey = strings.hotkey.clone();
            }
            if unit.propernames.is_none() {
                unit.propernames = strings.propernames.clone();
            }
            if unit.revivetip.is_none() {
                unit.revivetip = strings.revivetip.clone();
            }
            if unit.awakentip.is_none() {
                unit.awakentip = strings.awakentip.clone();
            }
            if unit.editor_suffix.is_none() {
                unit.editor_suffix = strings.editor_suffix.clone();
            }
            if unit.caster_upgrade_name.is_none() {
                unit.caster_upgrade_name = strings.casterupgradename.clone();
            }
            if unit.caster_upgrade_tip.is_none() {
                unit.caster_upgrade_tip = strings.casterupgradetip.clone();
            }
        }
    }

    Some(UnitsSlkResult {
        source: source.to_string(),
        sources,
        units_default: HashMap::new(),
        units,
        unit_strings,
        w3u_errors: Vec::new(),
    })
}

// ─── UnitMetaData.slk ────────────────────────────────────────────────────────

/// A single entry from `Units\UnitMetaData.slk`.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UnitMetaEntry {
    pub id: String,
    pub field: String,
    pub meta_type: String,
}

pub type UnitMetaMap = HashMap<String, UnitMetaEntry>;

/// Load and parse the embedded `Units\UnitMetaData.slk`.
pub fn load_unit_metadata() -> UnitMetaMap {
    let data = include_bytes!("../../../lng/slk/fixtures/Units/UnitMetaData.slk");
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
        map.insert(id.clone(), UnitMetaEntry { id, field, meta_type });
    }
    map
}

// ─── Merge war3map.w3u into units ────────────────────────────────────────────

/// Apply `war3map.w3u` modifications to the units map.
///
/// Returns a list of human-readable error messages for problems encountered.
pub fn merge_w3u_into_units(
    units: &mut HashMap<u32, UnitInfo>,
    w3u: &W3ObjectData,
    meta: &UnitMetaMap,
) -> Vec<String> {
    let mut errors = Vec::new();
    let defaults = units.clone();

    // Process original-table modifications.
    for def in &w3u.table.originals {
        let key = def.original_id.raw;
        let base = match units.get_mut(&key) {
            Some(u) => u,
            None => {
                errors.push(format!(
                    "w3u original: base unit '{}' (0x{:08X}) not found in UnitData.slk",
                    def.original_id.text, key
                ));
                continue;
            }
        };
        base.w3u_modified = true;
        for set in &def.sets {
            for modif in &set.modifications {
                apply_unit_modification(base, &modif.modification_id.text, &modif.value, meta, &mut errors);
            }
        }
    }

    // Process custom-table entries.
    for def in &w3u.table.customs {
        let orig_key = def.original_id.raw;
        let base = match defaults.get(&orig_key) {
            Some(u) => u.clone(),
            None => {
                errors.push(format!(
                    "w3u custom: base unit '{}' (0x{:08X}) not found in UnitData.slk for custom '{}'",
                    def.original_id.text, orig_key, def.custom_id.text
                ));
                continue;
            }
        };

        let mut custom = base;
        custom.base_id = def.original_id.text.clone();
        custom.unit_id = def.custom_id.text.clone();
        custom.w3u_modified = true;
        custom.defaults = HashMap::new();

        for set in &def.sets {
            for modif in &set.modifications {
                apply_unit_modification(&mut custom, &modif.modification_id.text, &modif.value, meta, &mut errors);
            }
        }

        let custom_key = def.custom_id.raw;
        units.insert(custom_key, custom);
    }

    errors
}

/// Apply a single modification to a `UnitInfo`.
fn apply_unit_modification(
    unit: &mut UnitInfo,
    mod_id: &str,
    value: &ModificationValue,
    meta: &UnitMetaMap,
    errors: &mut Vec<String>,
) {
    let entry = match meta.get(mod_id) {
        Some(e) => e,
        None => {
            errors.push(format!(
                "w3u: unknown modification rawcode '{}' for unit '{}'",
                mod_id, unit.unit_id
            ));
            return;
        }
    };

    let field = entry.field.as_str();
    match field {
        "Name" => {
            if let Some(s) = mod_value_string(value) {
                unit.defaults.entry("name".into()).or_insert_with(|| unit.name.value.clone());
                unit.name = crate::lng::map_editor::westrings::resolve_game_string(&s);
            }
        }
        "file" => {
            if let Some(s) = mod_value_string(value) {
                unit.defaults.entry("file".into()).or_insert_with(|| unit.file.clone());
                unit.file = s;
            }
        }
        "race" => {
            if let Some(s) = mod_value_string(value) {
                unit.defaults.entry("race".into()).or_insert_with(|| unit.race.clone());
                unit.race = s;
            }
        }
        "unitSound" => {
            if let Some(s) = mod_value_string(value) {
                unit.defaults.entry("unitSound".into()).or_insert_with(|| unit.unit_sound.clone());
                unit.unit_sound = s;
            }
        }
        "modelScale" => {
            if let Some(v) = mod_value_f64(value) {
                unit.defaults.entry("modelScale".into()).or_insert_with(|| unit.model_scale.to_string());
                unit.model_scale = v;
            }
        }
        "scale" => {
            if let Some(v) = mod_value_f64(value) {
                unit.defaults.entry("scale".into()).or_insert_with(|| unit.scale.to_string());
                unit.scale = v;
            }
        }
        "HP" => {
            if let Some(v) = mod_value_u32(value) {
                unit.defaults.entry("hp".into()).or_insert_with(|| unit.hp.to_string());
                unit.hp = v;
            }
        }
        "realHP" => {
            if let Some(v) = mod_value_f64(value) {
                unit.defaults.entry("realHp".into()).or_insert_with(|| unit.real_hp.to_string());
                unit.real_hp = v;
            }
        }
        "def" => {
            if let Some(v) = mod_value_u32(value) {
                unit.defaults.entry("def".into()).or_insert_with(|| unit.def.to_string());
                unit.def = v;
            }
        }
        "defType" => {
            if let Some(s) = mod_value_string(value) {
                unit.defaults.entry("defType".into()).or_insert_with(|| unit.def_type.clone());
                unit.def_type = s;
            }
        }
        "spd" => {
            if let Some(v) = mod_value_u32(value) {
                unit.defaults.entry("spd".into()).or_insert_with(|| unit.spd.to_string());
                unit.spd = v;
            }
        }
        "level" => {
            if let Some(v) = mod_value_u32(value) {
                unit.defaults.entry("level".into()).or_insert_with(|| unit.level.to_string());
                unit.level = v;
            }
        }
        "sight" => {
            if let Some(v) = mod_value_u32(value) {
                unit.defaults.entry("sight".into()).or_insert_with(|| unit.sight.to_string());
                unit.sight = v;
            }
        }
        "nsight" => {
            if let Some(v) = mod_value_u32(value) {
                unit.defaults.entry("nsight".into()).or_insert_with(|| unit.nsight.to_string());
                unit.nsight = v;
            }
        }
        "goldcost" => {
            if let Some(v) = mod_value_u32(value) {
                unit.defaults.entry("goldCost".into()).or_insert_with(|| unit.gold_cost.to_string());
                unit.gold_cost = v;
            }
        }
        "lumbercost" => {
            if let Some(v) = mod_value_u32(value) {
                unit.defaults.entry("lumberCost".into()).or_insert_with(|| unit.lumber_cost.to_string());
                unit.lumber_cost = v;
            }
        }
        "bldtm" => {
            if let Some(v) = mod_value_u32(value) {
                unit.defaults.entry("bldTm".into()).or_insert_with(|| unit.bld_tm.to_string());
                unit.bld_tm = v;
            }
        }
        "collision" => {
            if let Some(v) = mod_value_f64(value) {
                unit.defaults.entry("collision".into()).or_insert_with(|| unit.collision.to_string());
                unit.collision = v;
            }
        }
        "pathTex" => {
            if let Some(s) = mod_value_string(value) {
                unit.defaults.entry("pathTex".into()).or_insert_with(|| unit.path_tex.clone());
                unit.path_tex = s;
            }
        }
        "tilesets" => {
            if let Some(s) = mod_value_string(value) {
                unit.defaults.entry("tilesets".into()).or_insert_with(|| unit.tilesets.clone());
                unit.tilesets = s;
            }
        }
        "isbldg" => {
            if let Some(b) = mod_value_bool(value) {
                unit.defaults.entry("isBldg".into()).or_insert_with(|| unit.is_bldg.to_string());
                unit.is_bldg = b;
            }
        }
        "maxPitch" => {
            if let Some(v) = mod_value_f64(value) {
                unit.defaults.entry("maxPitch".into()).or_insert_with(|| unit.max_pitch.to_string());
                unit.max_pitch = v;
            }
        }
        "maxRoll" => {
            if let Some(v) = mod_value_f64(value) {
                unit.defaults.entry("maxRoll".into()).or_insert_with(|| unit.max_roll.to_string());
                unit.max_roll = v;
            }
        }
        "Tip" => {
            if let Some(s) = mod_value_string(value) {
                unit.tip = Some(s);
            }
        }
        "Ubertip" => {
            if let Some(s) = mod_value_string(value) {
                unit.ubertip = Some(s);
            }
        }
        "Hotkey" => {
            if let Some(s) = mod_value_string(value) {
                unit.hotkey = Some(s);
            }
        }
        "Propernames" => {
            if let Some(s) = mod_value_string(value) {
                unit.propernames = Some(s);
            }
        }
        // Many more fields exist in UnitMetaData.slk — handle key ones,
        // silently skip the rest for now.
        _ => {}
    }
}

