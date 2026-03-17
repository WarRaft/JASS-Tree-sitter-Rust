//! Binary reader for Warcraft III placement files (`.doo`).
//!
//! The `.doo` format describes unit placement ("war3mapUnits.doo") and
//! doodad/destructible placement ("war3map.doo").  The format is described
//! in `doo.hexpat` (ImHex pattern) and at
//! <https://xgm.guru/p/wc3/w3-file-format>.
//!
//! All multi-byte integers are **little-endian**.

pub mod send;

use crate::util::bin_reader::{BinRead, BinReader, BinReaderMeta, BinResult, Rawcode};
use serde::Serialize;

// ─── Simple types ────────────────────────────────────────────────────────────

crate::bin_struct! {
    /// 3-D vector (hexpat `struct Vector`).
    pub Vector { x: f32, y: f32, z: f32 }
}

crate::bin_enum! {
    /// Doodad visibility / collision flag (hexpat `enum Flag : u8`).
    pub Flag: u8 {
        Invisible = 0,
        Visible = 1,
        Normal = 2,
    }
}

crate::bin_struct! {
    /// A single dropped item entry (hexpat `struct DropItem`).
    pub DropItem { rawcode: Rawcode, chance: u32 }
}

crate::bin_struct! {
    /// A cliff / terrain decoration (hexpat `struct Cliff`).
    pub Cliff { rawcode: Rawcode, variation: u32, x: u32, y: u32 }
}

crate::bin_struct! {
    /// Data specific to a **unit** placement.
    pub UnitExtra { player: u32 }
}

// ─── Drop set (counted Vec) ─────────────────────────────────────────────────

/// One set of droppable items (hexpat `struct Drop`).
#[derive(Debug, Clone, Serialize)]
pub struct Drop {
    pub items: Vec<DropItem>,
}

impl BinRead for Drop {
    fn bin_read(r: &mut BinReader) -> BinResult<Self> {
        Ok(Self { items: r.read_vec()? })
    }
}

// ─── DoodadExtra (conditional fields) ────────────────────────────────────────

/// Data specific to a **doodad / destructible** placement.
#[derive(Debug, Clone, Serialize)]
pub struct DoodadExtra {
    pub health: u8,
    pub drop_index: Option<i32>,
    pub drops: Option<Vec<Drop>>,
    pub num: u32,
}

// ─── Placed item (unit or doodad) ────────────────────────────────────────────

/// A single placed object (unit or doodad/destructible).
#[derive(Debug, Clone, Serialize)]
pub struct DooItem {
    pub rawcode: Rawcode,
    pub variation: u32,
    pub position: Vector,
    /// Angle in radians.
    pub angle: f32,
    pub scale: Vector,
    /// Skin override (patch ≥ 32).
    pub skin: Option<u32>,
    pub flag: Flag,
    /// `Some(UnitExtra)` when parsing a units file, `None` for doodads.
    pub unit: Option<UnitExtra>,
    /// `Some(DoodadExtra)` when parsing a doodads file, `None` for units.
    pub doodad: Option<DoodadExtra>,
}

impl DooItem {
    fn read(r: &mut BinReader, is_unit: bool, format: u32, patch: u32) -> BinResult<Self> {
        let rawcode = Rawcode::bin_read(r)?;
        let variation = r.read_u32()?;
        let position = Vector::bin_read(r)?;
        let angle = r.read_f32()?;
        let scale = Vector::bin_read(r)?;

        let skin = if patch >= 32 { Some(r.read_u32()?) } else { None };
        let flag = Flag::bin_read(r)?;

        let (unit, doodad) = if is_unit {
            (Some(UnitExtra::bin_read(r)?), None)
        } else {
            let health = r.read_u8()?;
            let (drop_index, drops) = if format == 8 {
                let drop_index = r.read_s32()?;
                let drops = r.read_vec()?;
                (Some(drop_index), Some(drops))
            } else {
                (None, None)
            };
            let num = r.read_u32()?;
            (None, Some(DoodadExtra { health, drop_index, drops, num }))
        };

        Ok(Self { rawcode, variation, position, angle, scale, skin, flag, unit, doodad })
    }
}

// ─── Top-level DOO data ──────────────────────────────────────────────────────

/// Parsed contents of a `.doo` file.
#[derive(Debug, Clone, Serialize)]
pub struct DooData {
    pub magic: String,
    pub format: u32,
    pub subformat: u32,
    pub items: Vec<DooItem>,
    /// Cliff decorations — only present in doodad (non-unit) files.
    pub cliffs: Option<Vec<Cliff>>,
}

impl DooData {
    /// Parse a `.doo` file from raw bytes.
    ///
    /// * `is_unit` — `true` for `war3mapUnits.doo`, `false` for `war3map.doo`.
    /// * `patch`   — patch version from the parent `.w3i` (affects field layout).
    pub fn read(data: &[u8], is_unit: bool, patch: u32) -> BinResult<(Self, BinReaderMeta)> {
        let mut r = BinReader::new(data);

        let magic = r.read_fixed_string(4)?;  // "W3do"
        let format = r.read_u32()?;           // ROC=7, TFT=8
        let subformat = r.read_u32()?;

        let item_count = r.read_u32()?;
        let mut items = Vec::with_capacity(item_count as usize);
        for _ in 0..item_count {
            items.push(DooItem::read(&mut r, is_unit, format, patch)?);
        }

        let cliffs = if !is_unit {
            let _cliff_version = r.read_u32()?;
            let cliffs = r.read_vec()?;
            Some(cliffs)
        } else {
            None
        };

        let meta = r.meta();
        Ok((DooData { magic, format, subformat, items, cliffs }, meta))
    }
}
