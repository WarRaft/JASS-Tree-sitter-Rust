//! Map data: reading and code generation.
//!
//! Reads `war3map.w3i`, `war3mapUnits.doo`, and `war3map.doo` from an MPQ
//! archive, then augments the build IR by generating `config()` and `main()`
//! body statements for player setup, teams, camera bounds, DNC, fog, sound,
//! and unit/item/destructable creation.
//!
//! **Future direction — direct binary→AS**: currently the augmentation
//! functions emit JASS-flavoured IR statements (e.g. `IRExpr::rawcode`,
//! `IRExpr::call("CreateUnit", …)`).  When the binary→AS build path is
//! implemented, these functions should accept a `BuildMode` parameter and
//! emit AS-specific IR nodes directly (e.g. different function signatures,
//! AS-specific type constructors) so no intermediate JASS rendering is needed.

use super::ir::*;
use std::collections::HashSet;
use std::path::Path;

/// Parsed map data extracted from a `.w3x` / `.w3m` archive.
pub(super) struct MapData {
    pub w3i: crate::lng::w3i::W3iData,
    /// Unit placement data (`war3mapUnits.doo`).  `None` if not present.
    pub units_doo: Option<crate::lng::doo::parse::DooData>,
    /// Doodad / destructable placement data (`war3map.doo`).  `None` if not present.
    pub doodads_doo: Option<crate::lng::doo::parse::DooData>,
}

/// Read `war3map.w3i`, `war3mapUnits.doo`, and `war3map.doo` from an MPQ archive.
pub(super) fn read_map_data(archive_path: &Path) -> Result<MapData, String> {
    let archive = storm_rs::MpqArchive::open(archive_path)
        .map_err(|e| crate::util::i18n::build_archive_open_failed(
            &archive_path.display().to_string(),
            &e.to_string(),
        ))?;

    // Read w3i — required.
    let w3i_buf = archive.read_file("war3map.w3i")
        .map_err(|e| format!("Cannot read war3map.w3i: {e}"))?;
    let (w3i, _meta) = crate::lng::w3i::W3iData::read(&w3i_buf)
        .map_err(|e| format!("Cannot parse war3map.w3i: {e}"))?;

    // Determine the patch version for doo parsing.
    let patch = w3i.editor_version_full
        .map(|v| v[0])
        .unwrap_or(w3i.format);


    // Read units doo — optional.
    let units_doo = archive.read_file("war3mapUnits.doo")
        .ok()
        .and_then(|buf| crate::lng::doo::parse::DooData::read(&buf, true, patch).ok())
        .map(|(data, _meta)| data);

    // Read doodads / destructables doo — optional.
    let doodads_doo = archive.read_file("war3map.doo")
        .ok()
        .and_then(|buf| crate::lng::doo::parse::DooData::read(&buf, false, patch).ok())
        .map(|(data, _meta)| data);

    Ok(MapData { w3i, units_doo, doodads_doo })
}

// ─── JASS code generation helpers ────────────────────────────────────────────

/// Convert a Race enum to the JASS `RACE_PREF_*` constant.
fn race_to_jass(race: &crate::lng::w3i::Race) -> &'static str {
    match race {
        crate::lng::w3i::Race::Human => "RACE_PREF_HUMAN",
        crate::lng::w3i::Race::Orc => "RACE_PREF_ORC",
        crate::lng::w3i::Race::Undead => "RACE_PREF_UNDEAD",
        crate::lng::w3i::Race::NightElf => "RACE_PREF_NIGHTELF",
        crate::lng::w3i::Race::Random | _ => "RACE_PREF_RANDOM",
    }
}

/// Convert a PlayerType enum to the JASS `MAP_CONTROL_*` constant.
fn player_type_to_jass(pt: &crate::lng::w3i::PlayerType) -> &'static str {
    match pt {
        crate::lng::w3i::PlayerType::Human => "MAP_CONTROL_USER",
        crate::lng::w3i::PlayerType::Comp => "MAP_CONTROL_COMPUTER",
        crate::lng::w3i::PlayerType::Neutral => "MAP_CONTROL_NEUTRAL",
        crate::lng::w3i::PlayerType::Reserve | _ => "MAP_CONTROL_RESCUABLE",
    }
}

/// Map the w3i `land` tileset byte to (DNC terrain model, DNC unit model,
/// ambient day sound, ambient night sound).
fn tileset_env(land: u8) -> (&'static str, &'static str, &'static str, &'static str) {
    let dnc_lord_t = "Environment\\DNC\\DNCLordaeron\\DNCLordaeronTerrain\\DNCLordaeronTerrain.mdl";
    let dnc_lord_u = "Environment\\DNC\\DNCLordaeron\\DNCLordaeronUnit\\DNCLordaeronUnit.mdl";
    let dnc_dung_t = "Environment\\DNC\\DNCDungeon\\DNCDungeonTerrain\\DNCDungeonTerrain.mdl";
    let dnc_dung_u = "Environment\\DNC\\DNCDungeon\\DNCDungeonUnit\\DNCDungeonUnit.mdl";
    match land {
        b'A' => (dnc_lord_t, dnc_lord_u, "AshenvalDay",          "AshenvalNight"),
        b'B' => (dnc_lord_t, dnc_lord_u, "BarrensDay",           "BarrensNight"),
        b'C' => (dnc_lord_t, dnc_lord_u, "FelwoodDay",           "FelwoodNight"),
        b'D' => (dnc_dung_t, dnc_dung_u, "DungeonDay",           "DungeonNight"),
        b'F' => (dnc_lord_t, dnc_lord_u, "LordaeronFallDay",     "LordaeronFallNight"),
        b'G' => (dnc_dung_t, dnc_dung_u, "DungeonDay",           "DungeonNight"),
        b'I' => (dnc_lord_t, dnc_lord_u, "IcecrownDay",          "IcecrownNight"),
        b'J' => (dnc_lord_t, dnc_lord_u, "DalaranRuinsDay",      "DalaranRuinsNight"),
        b'K' => (dnc_lord_t, dnc_lord_u, "BlackCitadelDay",      "BlackCitadelNight"),
        b'L' => (dnc_lord_t, dnc_lord_u, "LordaeronSummerDay",   "LordaeronSummerNight"),
        b'N' => (dnc_lord_t, dnc_lord_u, "NorthrendDay",         "NorthrendNight"),
        b'O' => (dnc_lord_t, dnc_lord_u, "OutlandDay",           "OutlandNight"),
        b'Q' => (dnc_lord_t, dnc_lord_u, "LordaeronFallDay",     "LordaeronFallNight"),
        b'V' => (dnc_lord_t, dnc_lord_u, "VillageDay",           "VillageNight"),
        b'W' => (dnc_lord_t, dnc_lord_u, "CityScapeDay",         "CityScapeNight"),
        b'X' => (dnc_lord_t, dnc_lord_u, "DalaranDay",           "DalaranNight"),
        b'Y' => (dnc_lord_t, dnc_lord_u, "CityScapeDay",         "CityScapeNight"),
        b'Z' => (dnc_lord_t, dnc_lord_u, "SunkenRuinsDay",       "SunkenRuinsNight"),
        _    => (dnc_lord_t, dnc_lord_u, "LordaeronSummerDay",   "LordaeronSummerNight"),
    }
}

// ─── Map data → IR augmentation ──────────────────────────────────────────────

/// Augment the `config` function body with player slots, teams, and
/// ally priorities from `war3map.w3i`.
pub(super) fn augment_config(ir: &mut BuildIR, md: &MapData) {
    let func = ir.functions.get_mut("config").expect("config must exist");
    let body = &mut func.body;
    let w = &md.w3i;
    let fixed_settings = w.map_flags.fixed_player_settings();

    // Map name & description.
    body.push(IRStmt::call("SetMapName", vec![IRExpr::string(&w.map_name)]));
    body.push(IRStmt::call("SetMapDescription", vec![IRExpr::string(&w.description)]));

    // Players / teams / placement.
    body.push(IRStmt::call("SetPlayers", vec![IRExpr::int(w.players.len())]));
    if w.map_flags.custom_forces() {
        body.push(IRStmt::call("SetTeams", vec![IRExpr::int(w.forces.len())]));
        body.push(IRStmt::call("SetGamePlacement", vec![IRExpr::id("MAP_PLACEMENT_USE_MAP_SETTINGS")]));
    } else {
        body.push(IRStmt::call("SetTeams", vec![IRExpr::int(w.players.len())]));
        body.push(IRStmt::call("SetGamePlacement", vec![IRExpr::id("MAP_PLACEMENT_TEAMS_TOGETHER")]));
    }

    // Start locations.
    for (i, p) in w.players.iter().enumerate() {
        body.push(IRStmt::call("DefineStartLocation", vec![
            IRExpr::int(i), IRExpr::float1(p.start_position.x), IRExpr::float1(p.start_position.y),
        ]));
    }

    // ── Player slots ─────────────────────────────────────────
    for (i, p) in w.players.iter().enumerate() {
        let idx = p.num;
        let player = IRExpr::call("Player", vec![IRExpr::int(idx)]);
        body.push(IRStmt::call("SetPlayerStartLocation", vec![player.clone(), IRExpr::int(i)]));
        if p.fixed_start_position != 0 {
            body.push(IRStmt::call("ForcePlayerStartLocation", vec![player.clone(), IRExpr::int(i)]));
        }
        body.push(IRStmt::call("SetPlayerColor", vec![
            player.clone(), IRExpr::call("ConvertPlayerColor", vec![IRExpr::int(idx)]),
        ]));
        body.push(IRStmt::call("SetPlayerRacePreference", vec![
            player.clone(), IRExpr::id(race_to_jass(&p.race)),
        ]));
        let race_selectable = if fixed_settings {
            matches!(p.race, crate::lng::w3i::Race::Random)
        } else {
            true
        };
        body.push(IRStmt::call("SetPlayerRaceSelectable", vec![
            player.clone(), IRExpr::bool_val(race_selectable),
        ]));
        body.push(IRStmt::call("SetPlayerController", vec![
            player, IRExpr::id(player_type_to_jass(&p.player_type)),
        ]));
    }

    // ── Teams ────────────────────────────────────────────────
    let defined_players: HashSet<u32> = w.players.iter().map(|p| p.num).collect();
    for (i, clan) in w.forces.iter().enumerate() {
        let mask = clan.player_mask;
        for bit in 0..32u32 {
            if mask & (1 << bit) != 0 && defined_players.contains(&bit) {
                body.push(IRStmt::call("SetPlayerTeam", vec![
                    IRExpr::call("Player", vec![IRExpr::int(bit)]), IRExpr::int(i),
                ]));
            }
        }
        if clan.flags.allied() {
            for bit in 0..32u32 {
                if mask & (1 << bit) != 0 && defined_players.contains(&bit) {
                    for bit2 in 0..32u32 {
                        if bit != bit2 && mask & (1 << bit2) != 0 && defined_players.contains(&bit2) {
                            body.push(IRStmt::call("SetPlayerAllianceStateAllyBJ", vec![
                                IRExpr::call("Player", vec![IRExpr::int(bit)]),
                                IRExpr::call("Player", vec![IRExpr::int(bit2)]),
                                IRExpr::bool_val(true),
                            ]));
                        }
                    }
                }
            }
        }
        if clan.flags.shared_vision() {
            for bit in 0..32u32 {
                if mask & (1 << bit) != 0 && defined_players.contains(&bit) {
                    for bit2 in 0..32u32 {
                        if bit != bit2 && mask & (1 << bit2) != 0 && defined_players.contains(&bit2) {
                            body.push(IRStmt::call("SetPlayerAllianceStateVisionBJ", vec![
                                IRExpr::call("Player", vec![IRExpr::int(bit)]),
                                IRExpr::call("Player", vec![IRExpr::int(bit2)]),
                                IRExpr::bool_val(true),
                            ]));
                        }
                    }
                }
            }
        }
    }

    // ── Ally priorities ──────────────────────────────────────
    for (loc, p) in w.players.iter().enumerate() {
        let low = p.ally_priority_low.raw;
        let high = p.ally_priority_high.raw;
        if low == 0 && high == 0 { continue; }
        let mut entries: Vec<(usize, &str)> = Vec::new();
        for (other_loc, other_p) in w.players.iter().enumerate() {
            if other_loc == loc { continue; }
            let bit = other_p.num;
            if high & (1 << bit) != 0 {
                entries.push((other_loc, "MAP_LOC_PRIO_HIGH"));
            } else if low & (1 << bit) != 0 {
                entries.push((other_loc, "MAP_LOC_PRIO_LOW"));
            }
        }
        if entries.is_empty() { continue; }
        body.push(IRStmt::call("SetStartLocPrioCount", vec![IRExpr::int(loc), IRExpr::int(entries.len())]));
        for (slot, (target_loc, prio)) in entries.iter().enumerate() {
            body.push(IRStmt::call("SetStartLocPrio", vec![
                IRExpr::int(loc), IRExpr::int(slot), IRExpr::int(*target_loc), IRExpr::id(*prio),
            ]));
        }
    }
}

/// Augment the `main` function body with camera, DNC, fog, sound setup
/// and destructable/unit/item creation from map data.
pub(super) fn augment_main(ir: &mut BuildIR, md: &MapData) {
    let func = ir.functions.get_mut("main").expect("main must exist");
    let w = &md.w3i;

    // We'll collect locals and body separately, then prepend locals + body
    // to the existing function body (before user statements / bare_stmts).
    let mut locals: Vec<IRStmt> = Vec::new();
    let mut stmts: Vec<IRStmt> = Vec::new();

    // ── SetCameraBounds ──────────────────────────────────────
    // Camera bounds are computed from map dimensions and non-playable margins,
    // NOT from the cam_bounds field in w3i (which is the editor viewport).
    // Formula: left  = 64*(A - E - B),  right = 64*(A + E - B)
    //          bottom= 64*(C - F - D),  top   = 64*(C + F - D)
    // where A,B,C,D = map_size margins, E = map_width, F = map_height.
    let a = w.non_playable_margins[0] as f32;
    let b = w.non_playable_margins[1] as f32;
    let c = w.non_playable_margins[2] as f32;
    let d = w.non_playable_margins[3] as f32;
    let e = w.playable_width as f32;
    let f = w.playable_height as f32;

    let cam_left   = 64.0 * (a - e - b);
    let cam_bottom = 64.0 * (c - f - d);
    let cam_right  = 64.0 * (a + e - b);
    let cam_top    = 64.0 * (c + f - d);

    stmts.push(IRStmt::call("SetCameraBounds", vec![
        IRExpr::binary(IRExpr::float1(cam_left),   "+", IRExpr::call("GetCameraMargin", vec![IRExpr::id("CAMERA_MARGIN_LEFT")])),
        IRExpr::binary(IRExpr::float1(cam_bottom), "+", IRExpr::call("GetCameraMargin", vec![IRExpr::id("CAMERA_MARGIN_BOTTOM")])),
        IRExpr::binary(IRExpr::float1(cam_right),  "-", IRExpr::call("GetCameraMargin", vec![IRExpr::id("CAMERA_MARGIN_RIGHT")])),
        IRExpr::binary(IRExpr::float1(cam_top),    "-", IRExpr::call("GetCameraMargin", vec![IRExpr::id("CAMERA_MARGIN_TOP")])),
        IRExpr::binary(IRExpr::float1(cam_left),   "+", IRExpr::call("GetCameraMargin", vec![IRExpr::id("CAMERA_MARGIN_LEFT")])),
        IRExpr::binary(IRExpr::float1(cam_top),    "-", IRExpr::call("GetCameraMargin", vec![IRExpr::id("CAMERA_MARGIN_TOP")])),
        IRExpr::binary(IRExpr::float1(cam_right),  "-", IRExpr::call("GetCameraMargin", vec![IRExpr::id("CAMERA_MARGIN_RIGHT")])),
        IRExpr::binary(IRExpr::float1(cam_bottom), "+", IRExpr::call("GetCameraMargin", vec![IRExpr::id("CAMERA_MARGIN_BOTTOM")])),
    ]));

    // Day/night cycle models & ambient sounds.
    let (dnc_terrain, dnc_unit, day_snd, night_snd) = tileset_env(w.tileset);
    stmts.push(IRStmt::call("SetDayNightModels", vec![
        IRExpr::string(dnc_terrain), IRExpr::string(dnc_unit),
    ]));

    // Fog.
    if let (Some(fog_type), Some(fog_start), Some(fog_end), Some(fog_density), Some(fog_color))
        = (w.fog_type, w.fog_z_start, w.fog_z_end, w.fog_density, w.fog_color)
    {
        if fog_type != 0 || fog_start != 0.0 || fog_end != 0.0 || fog_density != 0.0 {
            let r = ((fog_color >> 16) & 0xFF) as f32 / 255.0;
            let g = ((fog_color >> 8) & 0xFF) as f32 / 255.0;
            let b = (fog_color & 0xFF) as f32 / 255.0;
            stmts.push(IRStmt::call("SetTerrainFogEx", vec![
                IRExpr::int(fog_type), IRExpr::float1(fog_start), IRExpr::float1(fog_end),
                IRExpr::float3(fog_density), IRExpr::float3(r), IRExpr::float3(g), IRExpr::float3(b),
            ]));
        }
    }

    // Water tint.
    if let Some(wc) = w.water_tint_color {
        if w.map_flags.water_color_override() {
            stmts.push(IRStmt::call("SetWaterBaseColor", vec![
                IRExpr::int((wc >> 16) & 0xFF), IRExpr::int((wc >> 8) & 0xFF),
                IRExpr::int(wc & 0xFF), IRExpr::int((wc >> 24) & 0xFF),
            ]));
        }
    }

    stmts.push(IRStmt::call("NewSoundEnvironment", vec![IRExpr::string("Default")]));
    stmts.push(IRStmt::call("SetAmbientDaySound", vec![IRExpr::string(day_snd)]));
    stmts.push(IRStmt::call("SetAmbientNightSound", vec![IRExpr::string(night_snd)]));
    stmts.push(IRStmt::call("SetMapMusic", vec![IRExpr::string("Music"), IRExpr::bool_val(true), IRExpr::int(0)]));

    // ── Destructables (from war3map.doo) ─────────────────────
    let mut need_destr_local = false;
    if let Some(ref doo) = md.doodads_doo {
        for item in &doo.items {
            let de = match &item.doodad { Some(d) => d, None => continue };
            need_destr_local = true;
            stmts.push(IRStmt::set("d", IRExpr::call("CreateDestructable", vec![
                IRExpr::rawcode(&item.rawcode.0),
                IRExpr::float1(item.position.x), IRExpr::float1(item.position.y),
                IRExpr::float3(item.angle.to_degrees()), IRExpr::float3(item.scale.x),
                IRExpr::int(item.variation),
            ])));
            if de.health < 100 {
                let pct = de.health as f64 / 100.0;
                stmts.push(IRStmt::call("SetDestructableLife", vec![
                    IRExpr::id("d"),
                    IRExpr::binary(
                        IRExpr::Literal(format!("{:.2}", pct)),
                        "*",
                        IRExpr::call("GetDestructableLife", vec![IRExpr::id("d")]),
                    ),
                ]));
            }
        }
    }

    // ── Units / items (from war3mapUnits.doo) ────────────────
    let mut need_unit_local = false;
    if let Some(ref doo) = md.units_doo {
        for item in &doo.items {
            let ue = match &item.unit { Some(u) => u, None => continue };
            let rawcode = &item.rawcode.0;
            if rawcode == "sloc" { continue; }
            let first_char = rawcode.chars().next().unwrap_or('\0');
            let is_item = first_char == 'I' || first_char == 'i';

            if is_item {
                stmts.push(IRStmt::call("CreateItem", vec![
                    IRExpr::rawcode(rawcode),
                    IRExpr::float1(item.position.x), IRExpr::float1(item.position.y),
                ]));
            } else {
                let needs_var = ue.health != 0xFFFFFFFF
                    || ue.mana != 0xFFFFFFFF
                    || (ue.target >= 0.0 && ue.target != -1.0);

                let create = IRExpr::call("CreateUnit", vec![
                    IRExpr::call("Player", vec![IRExpr::int(ue.player)]),
                    IRExpr::rawcode(rawcode),
                    IRExpr::float1(item.position.x), IRExpr::float1(item.position.y),
                    IRExpr::float3(item.angle.to_degrees()),
                ]);

                if needs_var {
                    need_unit_local = true;
                    stmts.push(IRStmt::set("u", create));
                    if ue.health != 0xFFFFFFFF {
                        let pct = ue.health as f64 / 100.0;
                        if (pct - 1.0).abs() > 0.001 {
                            stmts.push(IRStmt::call("SetUnitState", vec![
                                IRExpr::id("u"), IRExpr::id("UNIT_STATE_LIFE"),
                                IRExpr::binary(
                                    IRExpr::Literal(format!("{:.2}", pct)),
                                    "*",
                                    IRExpr::call("GetUnitState", vec![
                                        IRExpr::id("u"), IRExpr::id("UNIT_STATE_LIFE"),
                                    ]),
                                ),
                            ]));
                        }
                    }
                    if ue.mana != 0xFFFFFFFF {
                        stmts.push(IRStmt::call("SetUnitState", vec![
                            IRExpr::id("u"), IRExpr::id("UNIT_STATE_MANA"), IRExpr::int(ue.mana),
                        ]));
                    }
                    if ue.target >= 0.0 {
                        stmts.push(IRStmt::call("SetUnitAcquireRange", vec![
                            IRExpr::id("u"), IRExpr::float1(ue.target as f32),
                        ]));
                    }
                } else {
                    stmts.push(IRStmt::Call { name: "CreateUnit".into(), args: match create {
                        IRExpr::Call { args, .. } => args,
                        _ => vec![],
                    }});
                }
            }
        }
    }

    stmts.push(IRStmt::call("InitBlizzard", vec![]));

    // Nullify locals.
    if need_destr_local {
        stmts.push(IRStmt::set("d", IRExpr::null()));
    }
    if need_unit_local {
        stmts.push(IRStmt::set("u", IRExpr::null()));
    }

    // Build locals list.
    if need_destr_local {
        locals.push(IRStmt::local("destructable", "d"));
    }
    if need_unit_local {
        locals.push(IRStmt::local("unit", "u"));
    }

    // Prepend locals + generated stmts before the existing body.
    locals.append(&mut stmts);
    let mut existing = std::mem::take(&mut func.body);
    locals.append(&mut existing);
    func.body = locals;
}

