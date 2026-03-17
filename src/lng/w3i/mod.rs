//! Binary reader for Warcraft III map information files (`.w3i`).
//!
//! The format is described in `w3i.hexpat` (ImHex pattern) and at
//! <https://xgm.guru/p/wc3/w3-file-format>.
//!
//! All multi-byte integers are **little-endian**.

pub mod send;

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
    /// Clan (force) flags (hexpat `bitfield ClanFlags`).
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
    pub Clan { flags: ClanFlags, players: u32, name: String }
}

crate::bin_struct! {
    /// Custom upgrade entry (hexpat `struct Upgrade`).
    pub Upgrade { players: PlayerBool, id: Rawcode, level: u32, status: UpgradeStatus }
}

crate::bin_struct! {
    /// Custom tech — disabled ability/unit/item (hexpat `struct Tech`).
    pub Tech { players: PlayerBool, id: Rawcode }
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
    pub fix: u32,
    pub name: String,
    pub pos: Point,
    pub priority_low: PlayerBool,
    pub priority_high: PlayerBool,
    /// Extra fields present in format ≥ 31.
    pub extra: Option<(u32, u32)>,
}

impl Player {
    fn read(r: &mut BinReader, format: u32) -> BinResult<Self> {
        Ok(Self {
            num: r.read_u32()?,
            player_type: PlayerType::bin_read(r)?,
            race: Race::bin_read(r)?,
            fix: r.read_u32()?,
            name: r.read_cstring()?,
            pos: Point::bin_read(r)?,
            priority_low: PlayerBool::bin_read(r)?,
            priority_high: PlayerBool::bin_read(r)?,
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
        let j = r.read_u32()?;
        let mut column_types = Vec::with_capacity(j as usize);
        for _ in 0..j {
            column_types.push(r.read_u32()?);
        }
        let k = r.read_u32()?;
        let mut chances = Vec::with_capacity(k as usize);
        for _ in 0..k {
            chances.push(Chance::read(r, j)?);
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
    pub players_description: String,
    pub cam_bounds: Rect,
    pub map_size: [i32; 4],
    pub map_width: u32,
    pub map_height: u32,
    pub map_flags: MapFlags,
    pub land: u8,
    pub loadscreen_num: i32,
    pub loadscreen_path: Option<String>,
    pub loadscreen_text: String,
    pub loadscreen_title: String,
    pub loadscreen_subtitle: String,
    pub game_data_set: u32,
    pub prologue_path: Option<String>,
    pub prologue_text: String,
    pub prologue_title: String,
    pub prologue_subtitle: String,
    // TFT+ fog & weather
    pub fog: Option<u32>,
    pub fog_start: Option<f32>,
    pub fog_end: Option<f32>,
    pub fog_density: Option<f32>,
    pub fog_color: Option<u32>,
    pub weather: Option<Rawcode>,
    pub sound: Option<String>,
    pub light: Option<u8>,
    pub water_color: Option<u32>,
    // 1.31+
    pub is_lua: Option<u32>,
    // players & forces
    pub players: Vec<Player>,
    pub clans: Vec<Clan>,
    pub upgrades: Vec<Upgrade>,
    pub techs: Vec<Tech>,
    pub groups: Vec<Group>,
    pub items: Option<Vec<Item>>,
}

impl W3iData {
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
        let players_description = r.read_cstring()?;

        let cam_bounds = Rect::bin_read(&mut r)?;
        let map_size = [r.read_s32()?, r.read_s32()?, r.read_s32()?, r.read_s32()?];
        let map_width = r.read_u32()?;
        let map_height = r.read_u32()?;
        let map_flags = MapFlags::bin_read(&mut r)?;
        let land = r.read_char()?;
        let loadscreen_num = r.read_s32()?;

        let loadscreen_path = if format >= 25 { Some(r.read_cstring()?) } else { None };
        let loadscreen_text = r.read_cstring()?;
        let loadscreen_title = r.read_cstring()?;
        let loadscreen_subtitle = r.read_cstring()?;

        let game_data_set = r.read_u32()?;

        let prologue_path = if format >= 25 { Some(r.read_cstring()?) } else { None };
        let prologue_text = r.read_cstring()?;
        let prologue_title = r.read_cstring()?;
        let prologue_subtitle = r.read_cstring()?;

        // TFT+ block
        let (fog, fog_start, fog_end, fog_density, fog_color, weather, sound, light, water_color) =
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

        let is_lua = if format >= 28 { Some(r.read_u32()?) } else { None };

        if format >= 31 { r.read_u32()?; r.read_u32()?; }
        if format >= 33 { r.read_u32()?; r.read_u32()?; r.read_u32()?; }

        // Players (need format param → manual loop)
        let player_count = r.read_u32()?;
        let mut players = Vec::with_capacity(player_count as usize);
        for _ in 0..player_count {
            players.push(Player::read(&mut r, format)?);
        }

        // Clans, Upgrades, Techs — simple BinRead types → read_vec
        let clans = r.read_vec()?;
        let upgrades = r.read_vec()?;
        let techs = r.read_vec()?;

        // Groups (Chance needs column param → manual)
        let group_count = r.read_u32()?;
        let mut groups = Vec::with_capacity(group_count as usize);
        for _ in 0..group_count {
            groups.push(Group::read(&mut r)?);
        }

        // Item tables (TFT+)
        let items = if format >= 25 {
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
            map_name, author, description, players_description,
            cam_bounds, map_size, map_width, map_height, map_flags,
            land, loadscreen_num, loadscreen_path,
            loadscreen_text, loadscreen_title, loadscreen_subtitle,
            game_data_set, prologue_path,
            prologue_text, prologue_title, prologue_subtitle,
            fog, fog_start, fog_end, fog_density, fog_color,
            weather, sound, light, water_color,
            is_lua,
            players, clans, upgrades, techs, groups, items,
        }, meta))
    }
}
