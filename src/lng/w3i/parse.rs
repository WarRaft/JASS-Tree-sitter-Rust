//! Binary reader for Warcraft III map information files (`.w3i`).
//!
//! The format is described in `w3i.hexpat` (ImHex pattern) and at
//! <https://xgm.guru/p/wc3/w3-file-format>.
//!
//! All multi-byte integers are **little-endian**.

use crate::util::bin_reader::{BinRead, BinReader, BinReaderMeta, BinResult, Rawcode};
use serde::Serialize;

// ─── Simple types (hexpat structs) ───────────────────────────────────────────

crate::bin_struct! {
    /// BGRA colour (hexpat `struct Color`).
    #[allow(dead_code)]
    pub Color { b: u8, g: u8, r: u8, a: u8 }
}

crate::bin_struct! {
    /// 2-D point (hexpat `struct Point`).
    pub Point { x: f32, y: f32 }
}

crate::bin_struct! {
    /// Camera bounds rectangle — 4 corners: lb, rt, lt, rb.
    pub Rect { lb: Point, rt: Point, lt: Point, rb: Point }
}

// ─── Bitfields ───────────────────────────────────────────────────────────────

crate::bin_bitfield! {
    /// Map flags (hexpat `bitfield MapFlags`).
    pub MapFlags: u32 {
        hide_minimap = 0,
        ally_priorities_edit = 1,
        melee_map = 2,
        custom_terrain_type = 3,
        partial_visibility = 4,
        fixed_player_settings = 5,
        custom_forces = 6,
        custom_techs = 7,
        custom_abilities = 8,
        custom_upgrades = 9,
        waves_steep_shore = 11,
        waves_shallow_shore = 12,
        terrain_fog = 13,
        expansion_required = 14,
        item_classification = 15,
        water_color_override = 16,
    }
}

crate::bin_bitfield! {
    /// 32-bit player mask (hexpat `bitfield PlayerBool`).
    pub PlayerBool: u32 {
        player1  = 0,  player2  = 1,  player3  = 2,  player4  = 3,
        player5  = 4,  player6  = 5,  player7  = 6,  player8  = 7,
        player9  = 8,  player10 = 9,  player11 = 10, player12 = 11,
        player13 = 12, player14 = 13, player15 = 14, player16 = 15,
        player17 = 16, player18 = 17, player19 = 18, player20 = 19,
        player21 = 20, player22 = 21, player23 = 22, player24 = 23,
        player25 = 24, player26 = 25, player27 = 26, player28 = 27,
        player29 = 28, player30 = 29, player31 = 30, player32 = 31,
    }
}

crate::bin_bitfield! {
    /// Force flags (hexpat `bitfield ClanFlags`).
    pub ClanFlags: u32 {
        allied = 0,
        shared_victory = 1,
        shared_vision = 3,
        shared_units = 4,
        shared_all_units = 5,
    }
}

// ─── Enums ───────────────────────────────────────────────────────────────────

crate::bin_enum! {
    /// Player type (hexpat `enum PlayerType : u32`).
    pub PlayerType: u32 {
        Human = 1,
        Comp = 2,
        Neutral = 3,
        Reserve = 4,
    }
}

crate::bin_enum! {
    /// Race (hexpat `enum Race : u32`).
    pub Race: u32 {
        Random = 0,
        Human = 1,
        Orc = 2,
        Undead = 3,
        NightElf = 4,
    }
}

crate::bin_enum! {
    /// Upgrade availability status.
    pub UpgradeStatus: u32 {
        Disabled = 0,
        Available = 1,
        Researched = 2,
    }
}

// ─── Simple composite structs ────────────────────────────────────────────────

crate::bin_struct! {
    /// Force / clan entry (hexpat `struct Clan`).
    pub Clan { flags: ClanFlags, player_mask: u32, name: String }
}

crate::bin_struct! {
    /// Custom upgrade entry (hexpat `struct Upgrade`).
    pub Upgrade { affected_players: PlayerBool, id: Rawcode, level: u32, status: UpgradeStatus }
}

crate::bin_struct! {
    /// Disabled tech entry (hexpat `struct Tech`).
    pub Tech { disabled_players: PlayerBool, id: Rawcode }
}

crate::bin_struct! {
    /// Single chance entry inside a random item table (hexpat `struct ItemChance`).
    pub ItemChance { chance: u32, id: Rawcode }
}

// ─── Structs with conditional / dynamic fields ──────────────────────────────

/// Single player entry (hexpat `struct Player`).
#[derive(Debug, Clone, Serialize)]
pub struct Player {
    pub num: u32,
    pub player_type: PlayerType,
    pub race: Race,
    pub fixed_start_position: u32,
    pub name: String,
    pub start_position: Point,
    pub ally_priority_low: PlayerBool,
    pub ally_priority_high: PlayerBool,
    /// Extra fields present in format ≥ 31.
    pub extra: Option<(u32, u32)>,
}

impl Player {
    fn read(r: &mut BinReader, format: u32) -> BinResult<Self> {
        Ok(Self {
            num: r.read_u32()?,
            player_type: PlayerType::bin_read(r)?,
            race: Race::bin_read(r)?,
            fixed_start_position: r.read_u32()?,
            name: r.read_cstring()?,
            start_position: Point::bin_read(r)?,
            ally_priority_low: PlayerBool::bin_read(r)?,
            ally_priority_high: PlayerBool::bin_read(r)?,
            extra: if format >= 31 {
                Some((r.read_u32()?, r.read_u32()?))
            } else {
                None
            },
        })
    }
}

/// Single chance entry inside a random group row (hexpat `struct Chance`).
#[derive(Debug, Clone, Serialize)]
pub struct Chance {
    pub chance: u32,
    /// Rawcodes, one per column.
    pub ids: Vec<Rawcode>,
}

impl Chance {
    fn read(r: &mut BinReader, columns: u32) -> BinResult<Self> {
        let chance = r.read_u32()?;
        let mut ids = Vec::with_capacity(columns as usize);
        for _ in 0..columns {
            ids.push(Rawcode::bin_read(r)?);
        }
        Ok(Self { chance, ids })
    }
}

/// Random unit/building/item group (hexpat `struct Group`).
#[derive(Debug, Clone, Serialize)]
pub struct Group {
    pub num: u32,
    pub name: String,
    pub column_types: Vec<u32>,
    pub chances: Vec<Chance>,
}

impl Group {
    fn read(r: &mut BinReader) -> BinResult<Self> {
        let num = r.read_u32()?;
        let name = r.read_cstring()?;
        let column_count = r.read_u32()?;
        let mut column_types = Vec::with_capacity(column_count as usize);
        for _ in 0..column_count {
            column_types.push(r.read_u32()?);
        }
        let row_count = r.read_u32()?;
        let mut chances = Vec::with_capacity(row_count as usize);
        for _ in 0..row_count {
            chances.push(Chance::read(r, column_count)?);
        }
        Ok(Self { num, name, column_types, chances })
    }
}

/// One set of item chances (hexpat `struct ItemGroup`).
#[derive(Debug, Clone, Serialize)]
pub struct ItemGroup {
    pub chances: Vec<ItemChance>,
}

impl BinRead for ItemGroup {
    fn bin_read(r: &mut BinReader) -> BinResult<Self> {
        Ok(Self { chances: r.read_vec()? })
    }
}

/// Random item table (hexpat `struct Item`).
#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub num: u32,
    pub name: String,
    pub groups: Vec<ItemGroup>,
}

impl Item {
    fn read(r: &mut BinReader) -> BinResult<Self> {
        Ok(Self {
            num: r.read_u32()?,
            name: r.read_cstring()?,
            groups: r.read_vec()?,
        })
    }
}

// ─── Top-level W3I data ──────────────────────────────────────────────────────

/// Parsed contents of a `.w3i` file.
#[derive(Debug, Clone, Serialize)]
pub struct W3iData {
    pub format: u32,
    pub save_count: u32,
    pub editor_version: u32,
    pub editor_version_full: Option<[u32; 4]>,
    pub map_name: String,
    pub author: String,
    pub description: String,
    pub recommended_players: String,
    pub camera_bounds: Rect,
    pub non_playable_margins: [i32; 4],
    pub playable_width: u32,
    pub playable_height: u32,
    pub map_flags: MapFlags,
    pub tileset: u8,
    pub loading_screen_preset: i32,
    pub loading_screen_model: Option<String>,
    pub loading_screen_text: String,
    pub loading_screen_title: String,
    pub loading_screen_subtitle: String,
    pub game_data_set: u32,
    pub prologue_screen_model: Option<String>,
    pub prologue_text: String,
    pub prologue_title: String,
    pub prologue_subtitle: String,
    // TFT+ fog & weather
    pub fog_type: Option<u32>,
    pub fog_z_start: Option<f32>,
    pub fog_z_end: Option<f32>,
    pub fog_density: Option<f32>,
    pub fog_color: Option<u32>,
    pub global_weather: Option<Rawcode>,
    pub ambient_sound: Option<String>,
    pub tileset_light: Option<u8>,
    pub water_tint_color: Option<u32>,
    // 1.31+
    pub script_language: Option<u32>,
    // players & forces
    pub players: Vec<Player>,
    pub forces: Vec<Clan>,
    // tail sections — may be absent in truncated files
    pub custom_upgrades_missing: bool,
    pub custom_upgrades: Vec<Upgrade>,
    pub disabled_techs_missing: bool,
    pub disabled_techs: Vec<Tech>,
    pub random_groups_missing: bool,
    pub random_groups: Vec<Group>,
    pub random_item_tables_missing: bool,
    pub random_item_tables: Option<Vec<Item>>,
    /// Unrecognised bytes remaining after all known sections were read.
    pub tail_bytes: Vec<u8>,
}

impl Default for W3iData {
    fn default() -> Self {
        Self {
            format: 0, save_count: 0, editor_version: 0,
            editor_version_full: None,
            map_name: String::new(), author: String::new(),
            description: String::new(), recommended_players: String::new(),
            camera_bounds: Rect {
                lb: Point { x: 0.0, y: 0.0 }, rt: Point { x: 0.0, y: 0.0 },
                lt: Point { x: 0.0, y: 0.0 }, rb: Point { x: 0.0, y: 0.0 },
            },
            non_playable_margins: [0; 4], playable_width: 0, playable_height: 0,
            map_flags: MapFlags { raw: 0 }, tileset: 0,
            loading_screen_preset: 0, loading_screen_model: None,
            loading_screen_text: String::new(), loading_screen_title: String::new(),
            loading_screen_subtitle: String::new(),
            game_data_set: 0, prologue_screen_model: None,
            prologue_text: String::new(), prologue_title: String::new(),
            prologue_subtitle: String::new(),
            fog_type: None, fog_z_start: None, fog_z_end: None, fog_density: None,
            fog_color: None, global_weather: None, ambient_sound: None, tileset_light: None,
            water_tint_color: None, script_language: None,
            players: Vec::new(), forces: Vec::new(),
            custom_upgrades_missing: true,
            custom_upgrades: Vec::new(),
            disabled_techs_missing: true,
            disabled_techs: Vec::new(),
            random_groups_missing: true,
            random_groups: Vec::new(),
            random_item_tables_missing: true,
            random_item_tables: None,
            tail_bytes: Vec::new(),
        }
    }
}

impl W3iData {
    /// Parse a `.w3i` file from raw bytes, returning partial data on error.
    ///
    /// Always returns a `W3iData` (with defaults for unread fields),
    /// `BinReaderMeta`, and an optional error message.
    pub fn read_partial(data: &[u8]) -> (Self, BinReaderMeta, Option<String>) {
        let mut r = BinReader::new(data);
        let mut d = Self::default();

        macro_rules! try_read {
            ($expr:expr) => {
                match $expr {
                    Ok(v) => v,
                    Err(e) => return (d, r.meta(), Some(e.to_string())),
                }
            };
        }

        d.format = try_read!(r.read_u32());
        d.save_count = try_read!(r.read_u32());
        d.editor_version = try_read!(r.read_u32());

        if d.format > 28 {
            d.editor_version_full = Some([
                try_read!(r.read_u32()), try_read!(r.read_u32()),
                try_read!(r.read_u32()), try_read!(r.read_u32()),
            ]);
        }

        d.map_name = try_read!(r.read_cstring());
        d.author = try_read!(r.read_cstring());
        d.description = try_read!(r.read_cstring());
        d.recommended_players = try_read!(r.read_cstring());

        d.camera_bounds = try_read!(Rect::bin_read(&mut r));
        d.non_playable_margins = [
            try_read!(r.read_s32()), try_read!(r.read_s32()),
            try_read!(r.read_s32()), try_read!(r.read_s32()),
        ];
        d.playable_width = try_read!(r.read_u32());
        d.playable_height = try_read!(r.read_u32());
        d.map_flags = try_read!(MapFlags::bin_read(&mut r));
        d.tileset = try_read!(r.read_char());
        d.loading_screen_preset = try_read!(r.read_s32());

        if d.format >= 25 { d.loading_screen_model = Some(try_read!(r.read_cstring())); }
        d.loading_screen_text = try_read!(r.read_cstring());
        d.loading_screen_title = try_read!(r.read_cstring());
        d.loading_screen_subtitle = try_read!(r.read_cstring());

        d.game_data_set = try_read!(r.read_u32());

        if d.format >= 25 { d.prologue_screen_model = Some(try_read!(r.read_cstring())); }
        d.prologue_text = try_read!(r.read_cstring());
        d.prologue_title = try_read!(r.read_cstring());
        d.prologue_subtitle = try_read!(r.read_cstring());

        // TFT+ block
        if d.format >= 25 {
            d.fog_type = Some(try_read!(r.read_u32()));
            d.fog_z_start = Some(try_read!(r.read_f32()));
            d.fog_z_end = Some(try_read!(r.read_f32()));
            d.fog_density = Some(try_read!(r.read_f32()));
            d.fog_color = Some(try_read!(r.read_u32()));
            d.global_weather = Some(try_read!(Rawcode::bin_read(&mut r)));
            d.ambient_sound = Some(try_read!(r.read_cstring()));
            d.tileset_light = Some(try_read!(r.read_char()));
            d.water_tint_color = Some(try_read!(r.read_u32()));
        }

        if d.format >= 28 { d.script_language = Some(try_read!(r.read_u32())); }

        if d.format >= 31 { try_read!(r.read_u32()); try_read!(r.read_u32()); }
        if d.format >= 33 { try_read!(r.read_u32()); try_read!(r.read_u32()); try_read!(r.read_u32()); }

        // Players
        let player_count = try_read!(r.read_u32());
        for _ in 0..player_count {
            d.players.push(try_read!(Player::read(&mut r, d.format)));
        }

        // Forces
        d.forces = try_read!(r.read_vec());

        // ── Tail sections — may be absent in truncated files ──────────────

        // Custom Upgrades
        if r.remaining() < 4 {
            d.tail_bytes = r.read_bytes(r.remaining()).unwrap_or(&[]).to_vec();
            return (d, r.meta(), None);
        }
        d.custom_upgrades_missing = false;
        d.custom_upgrades = try_read!(r.read_vec());

        // Disabled Techs
        if r.remaining() < 4 {
            d.tail_bytes = r.read_bytes(r.remaining()).unwrap_or(&[]).to_vec();
            return (d, r.meta(), None);
        }
        d.disabled_techs_missing = false;
        d.disabled_techs = try_read!(r.read_vec());

        // Random Groups
        if r.remaining() < 4 {
            d.tail_bytes = r.read_bytes(r.remaining()).unwrap_or(&[]).to_vec();
            return (d, r.meta(), None);
        }
        d.random_groups_missing = false;
        let group_count = try_read!(r.read_u32());
        for _ in 0..group_count {
            d.random_groups.push(try_read!(Group::read(&mut r)));
        }

        // Random Item Tables (TFT+)
        if d.format >= 25 {
            if r.remaining() < 4 {
                d.tail_bytes = r.read_bytes(r.remaining()).unwrap_or(&[]).to_vec();
                return (d, r.meta(), None);
            }
            d.random_item_tables_missing = false;
            let item_count = try_read!(r.read_u32());
            let mut items = Vec::with_capacity(item_count as usize);
            for _ in 0..item_count {
                items.push(try_read!(Item::read(&mut r)));
            }
            d.random_item_tables = Some(items);
        } else {
            d.random_item_tables_missing = false;
        }

        // Collect any remaining unrecognised bytes.
        if r.remaining() > 0 {
            d.tail_bytes = r.read_bytes(r.remaining()).unwrap_or(&[]).to_vec();
        }

        (d, r.meta(), None)
    }

    /// Parse a `.w3i` file from raw bytes.
    pub fn read(data: &[u8]) -> BinResult<(Self, BinReaderMeta)> {
        let mut r = BinReader::new(data);

        let format = r.read_u32()?;
        let save_count = r.read_u32()?;
        let editor_version = r.read_u32()?;

        let editor_version_full = if format > 28 {
            Some([r.read_u32()?, r.read_u32()?, r.read_u32()?, r.read_u32()?])
        } else {
            None
        };

        let map_name = r.read_cstring()?;
        let author = r.read_cstring()?;
        let description = r.read_cstring()?;
        let recommended_players = r.read_cstring()?;

        let camera_bounds = Rect::bin_read(&mut r)?;
        let non_playable_margins = [r.read_s32()?, r.read_s32()?, r.read_s32()?, r.read_s32()?];
        let playable_width = r.read_u32()?;
        let playable_height = r.read_u32()?;
        let map_flags = MapFlags::bin_read(&mut r)?;
        let tileset = r.read_char()?;
        let loading_screen_preset = r.read_s32()?;

        let loading_screen_model = if format >= 25 { Some(r.read_cstring()?) } else { None };
        let loading_screen_text = r.read_cstring()?;
        let loading_screen_title = r.read_cstring()?;
        let loading_screen_subtitle = r.read_cstring()?;

        let game_data_set = r.read_u32()?;

        let prologue_screen_model = if format >= 25 { Some(r.read_cstring()?) } else { None };
        let prologue_text = r.read_cstring()?;
        let prologue_title = r.read_cstring()?;
        let prologue_subtitle = r.read_cstring()?;

        // TFT+ fog & weather
        let (fog_type, fog_z_start, fog_z_end, fog_density, fog_color, global_weather, ambient_sound, tileset_light, water_tint_color) =
            if format >= 25 {
                (
                    Some(r.read_u32()?),
                    Some(r.read_f32()?),
                    Some(r.read_f32()?),
                    Some(r.read_f32()?),
                    Some(r.read_u32()?),
                    Some(Rawcode::bin_read(&mut r)?),
                    Some(r.read_cstring()?),
                    Some(r.read_char()?),
                    Some(r.read_u32()?),
                )
            } else {
                (None, None, None, None, None, None, None, None, None)
            };

        let script_language = if format >= 28 { Some(r.read_u32()?) } else { None };

        if format >= 31 { r.read_u32()?; r.read_u32()?; }
        if format >= 33 { r.read_u32()?; r.read_u32()?; r.read_u32()?; }

        // Players (need format param → manual loop)
        let player_count = r.read_u32()?;
        let mut players = Vec::with_capacity(player_count as usize);
        for _ in 0..player_count {
            players.push(Player::read(&mut r, format)?);
        }

        // Forces, Custom Upgrades, Disabled Techs — simple BinRead types → read_vec
        let forces = r.read_vec()?;
        let custom_upgrades = r.read_vec()?;
        let disabled_techs = r.read_vec()?;

        // Random Groups (Chance needs column param → manual)
        let group_count = r.read_u32()?;
        let mut random_groups = Vec::with_capacity(group_count as usize);
        for _ in 0..group_count {
            random_groups.push(Group::read(&mut r)?);
        }

        // Random Item Tables (TFT+)
        let random_item_tables = if format >= 25 {
            let item_count = r.read_u32()?;
            let mut items = Vec::with_capacity(item_count as usize);
            for _ in 0..item_count {
                items.push(Item::read(&mut r)?);
            }
            Some(items)
        } else {
            None
        };

        let meta = r.meta();
        Ok((W3iData {
            format, save_count, editor_version, editor_version_full,
            map_name, author, description, recommended_players,
            camera_bounds, non_playable_margins, playable_width, playable_height, map_flags,
            tileset, loading_screen_preset, loading_screen_model,
            loading_screen_text, loading_screen_title, loading_screen_subtitle,
            game_data_set, prologue_screen_model,
            prologue_text, prologue_title, prologue_subtitle,
            fog_type, fog_z_start, fog_z_end, fog_density, fog_color,
            global_weather, ambient_sound, tileset_light, water_tint_color,
            script_language,
            players, forces,
            custom_upgrades_missing: false, custom_upgrades,
            disabled_techs_missing: false, disabled_techs,
            random_groups_missing: false, random_groups,
            random_item_tables_missing: false, random_item_tables,
            tail_bytes: Vec::new(),
        }, meta))
    }
}

