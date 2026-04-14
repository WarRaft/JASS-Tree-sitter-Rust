//! Binary reader for Warcraft III region files (`war3map.w3r`).
//!
//! Layout (all little-endian):
//!
//! | Offset | Type       | Description                                    |
//! |--------|------------|------------------------------------------------|
//! | 0x00   | `u32`      | Format version (5 = TFT)                       |
//! | 0x04   | `u32`      | Region count                                   |
//! | 0x08   | `Region[]` | Array of regions                                |
//!
//! Each **Region**:
//!
//! | Type       | Description                                         |
//! |------------|-----------------------------------------------------|
//! | `f32`      | Left bound (world units)                            |
//! | `f32`      | Bottom bound (world units)                          |
//! | `f32`      | Right bound (world units)                           |
//! | `f32`      | Top bound (world units)                             |
//! | `CString`  | Region display name (null-terminated)               |
//! | `u32`      | Region sequential number                            |
//! | `[u8; 4]`  | Weather effect rawcode (`\0\0\0\0` = none)          |
//! | `CString`  | Ambient sound name (null-terminated, empty = none)  |
//! | `Color`    | Minimap display colour (B, G, R, A)                 |

use crate::util::bin_reader::{BinReader, BinReaderMeta, BinResult, Rawcode};
use serde::Serialize;

// ─── Color (BGRA) ────────────────────────────────────────────────────────────

crate::bin_struct! {
    /// BGRA colour stored in region data.
    pub W3rColor { b: u8, g: u8, r: u8, a: u8 }
}

// ─── Region ──────────────────────────────────────────────────────────────────

/// A single map region.
#[derive(Debug, Clone, Serialize)]
pub struct W3rRegion {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
    pub name: String,
    pub num: u32,
    pub weather: Rawcode,
    pub ambient_sound: String,
    pub color: W3rColor,
}

impl W3rRegion {
    fn read(r: &mut BinReader) -> BinResult<Self> {
        let left = r.read_f32()?;
        let bottom = r.read_f32()?;
        let right = r.read_f32()?;
        let top = r.read_f32()?;
        let name = r.read_cstring()?;
        let num = r.read_u32()?;
        let weather = Rawcode::bin_read(r)?;
        let ambient_sound = r.read_cstring()?;
        let color = W3rColor::bin_read(r)?;
        Ok(Self { left, bottom, right, top, name, num, weather, ambient_sound, color })
    }
}

use crate::util::bin_reader::BinRead;

// ─── W3rData (top-level) ─────────────────────────────────────────────────────

/// Parsed contents of a `.w3r` file.
#[derive(Debug, Clone, Serialize)]
pub struct W3rData {
    pub format: u32,
    pub regions: Vec<W3rRegion>,
}

impl W3rData {
    /// Parse a `.w3r` file from raw bytes.
    pub fn read(data: &[u8]) -> BinResult<(Self, BinReaderMeta)> {
        let mut r = BinReader::new(data);

        let format = r.read_u32()?;
        let count = r.read_u32()?;

        let mut regions = Vec::with_capacity(count as usize);
        for _ in 0..count {
            regions.push(W3rRegion::read(&mut r)?);
        }

        let meta = r.meta();
        Ok((Self { format, regions }, meta))
    }
}

