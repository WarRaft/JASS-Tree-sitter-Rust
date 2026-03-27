//! Binary reader for Warcraft III terrain files (`.w3e`).
//!
//! The format is described in `w3e.hexpat` (ImHex pattern).
//!
//! All multi-byte integers are **little-endian**.

use crate::util::bin_reader::{BinRead, BinReader, BinReaderMeta, BinResult, Rawcode};
use serde::Serialize;

// ─── Point ───────────────────────────────────────────────────────────────────

/// A single terrain point (7 bytes).
#[derive(Debug, Clone, Serialize)]
pub struct W3ePoint {
    /// Ground height. 8192 (0x2000) = zero height.
    pub ground_height: u16,
    /// Water height (14 bits) and camera boundary flag (bit 14).
    pub water_height: u16,
    /// Whether this tile is on the camera boundary.
    pub edge_flag: bool,
    /// Ground texture index in the tileset list (lower 4 bits).
    pub ground_texture: u8,
    /// Ramp flag (bit 4).
    pub ramp: bool,
    /// Blight flag (bit 5).
    pub blight: bool,
    /// Water flag (bit 6).
    pub water: bool,
    /// Boundary flag (bit 7).
    pub boundary: bool,
    /// Ground texture variation (lower 5 bits).
    pub ground_variation: u8,
    /// Cliff variation (upper 3 bits).
    pub cliff_variation: u8,
    /// Layer height (lower 4 bits).  Gameplay-only, does not affect the mesh.
    #[serde(skip_serializing)]
    #[allow(dead_code)]
    pub layer_height: u8,
    /// Cliff texture (upper 4 bits).
    pub cliff_texture: u8,
}

impl W3ePoint {
    fn read(r: &mut BinReader) -> BinResult<Self> {
        let ground_height = r.read_u16()?;

        let water_raw = r.read_u16()?;
        let water_height = water_raw & 0x3FFF;
        let edge_flag = water_raw & 0x4000 != 0;

        let texture_flags = r.read_u8()?;
        let ground_texture = texture_flags & 0x0F;
        let ramp = texture_flags & 0x10 != 0;
        let blight = texture_flags & 0x20 != 0;
        let water = texture_flags & 0x40 != 0;
        let boundary = texture_flags & 0x80 != 0;

        let variation = r.read_u8()?;
        let ground_variation = variation & 0x1F;
        let cliff_variation = (variation & 0xE0) >> 5;

        let layer = r.read_u8()?;
        let layer_height = layer & 0x0F;
        let cliff_texture = (layer & 0xF0) >> 4;

        Ok(Self {
            ground_height,
            water_height,
            edge_flag,
            ground_texture,
            ramp,
            blight,
            water,
            boundary,
            ground_variation,
            cliff_variation,
            layer_height,
            cliff_texture,
        })
    }
}

// ─── Top-level W3E data ──────────────────────────────────────────────────────

/// Parsed contents of a `.w3e` file.
#[derive(Debug, Clone, Serialize)]
pub struct W3eData {
    pub magic: String,
    pub version: i32,
    pub tileset: String,
    pub custom_tileset: i32,
    pub ground_tiles: Vec<Rawcode>,
    pub cliff_tiles: Vec<Rawcode>,
    pub map_width: i32,
    pub map_height: i32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub points: Vec<W3ePoint>,
}

impl W3eData {
    /// Parse a `.w3e` file from raw bytes.
    pub fn read(data: &[u8]) -> BinResult<(Self, BinReaderMeta)> {
        let mut r = BinReader::new(data);

        let magic = r.read_fixed_string(4)?;
        let version = r.read_s32()?;
        let tileset = {
            let ch = r.read_u8()?;
            String::from(ch as char)
        };
        let custom_tileset = r.read_s32()?;

        let ground_tile_count = r.read_u32()?;
        let mut ground_tiles = Vec::with_capacity(ground_tile_count as usize);
        for _ in 0..ground_tile_count {
            ground_tiles.push(Rawcode::bin_read(&mut r)?);
        }

        let cliff_tile_count = r.read_u32()?;
        let mut cliff_tiles = Vec::with_capacity(cliff_tile_count as usize);
        for _ in 0..cliff_tile_count {
            cliff_tiles.push(Rawcode::bin_read(&mut r)?);
        }

        let map_width = r.read_s32()?;
        let map_height = r.read_s32()?;
        let offset_x = r.read_f32()?;
        let offset_y = r.read_f32()?;

        let point_count = (map_width as usize) * (map_height as usize);
        let mut points = Vec::with_capacity(point_count);
        for _ in 0..point_count {
            points.push(W3ePoint::read(&mut r)?);
        }

        let meta = r.meta();
        Ok((
            W3eData {
                magic,
                version,
                tileset,
                custom_tileset,
                ground_tiles,
                cliff_tiles,
                map_width,
                map_height,
                offset_x,
                offset_y,
                points,
            },
            meta,
        ))
    }
}

