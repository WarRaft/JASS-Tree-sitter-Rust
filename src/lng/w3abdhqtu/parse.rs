//! Binary reader for Warcraft III object-data files
//! (`.w3a`, `.w3b`, `.w3d`, `.w3h`, `.w3q`, `.w3t`, `.w3u`).
//!
//! The format is described in `w3abdhqtu.hexpat` (ImHex pattern).
//!
//! All multi-byte integers are **little-endian**.

use crate::util::bin_reader::{BinRead, BinReader, BinReaderMeta, BinResult, Rawcode};
use serde::Serialize;

// ─── Value type tag ──────────────────────────────────────────────────────────

crate::bin_enum! {
    /// Type of the modification value (hexpat `valueType`).
    pub ValueType: u32 {
        Int    = 0,
        Real   = 1,
        Unreal = 2,
        String = 3,
    }
}

// ─── Modification value (tagged union) ───────────────────────────────────────

/// The actual value stored in a modification entry.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ModificationValue {
    Int(i32),
    Real(f32),
    Unreal(f32),
    Str(String),
}

// ─── Modification ────────────────────────────────────────────────────────────

/// A single modification entry (hexpat `struct Modification`).
#[derive(Debug, Clone, Serialize)]
pub struct Modification {
    /// The 4-char rawcode identifying which field is modified.
    pub modification_id: Rawcode,
    /// The type tag of the value.
    pub value_type: ValueType,
    /// Level (only present in level-based formats: `.w3a`, `.w3d`, `.w3q`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// Data index / column (only present in level-based formats).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_index: Option<u32>,
    /// The modification value.
    pub value: ModificationValue,
    /// Terminator / trailing rawcode (usually the original-id echo or `\0\0\0\0`).
    pub terminator: Rawcode,
}

impl Modification {
    fn read(r: &mut BinReader, level_data: bool) -> BinResult<Self> {
        let modification_id = Rawcode::bin_read(r)?;
        let raw_type = r.read_u32()?;
        let value_type = match raw_type {
            0 => ValueType::Int,
            1 => ValueType::Real,
            2 => ValueType::Unreal,
            3 => ValueType::String,
            v => ValueType::Unknown(v),
        };

        let (level, data_index) = if level_data {
            (Some(r.read_u32()?), Some(r.read_u32()?))
        } else {
            (None, None)
        };

        let value = match raw_type {
            0 => ModificationValue::Int(r.read_s32()?),
            1 => ModificationValue::Real(r.read_f32()?),
            2 => ModificationValue::Unreal(r.read_f32()?),
            3 => ModificationValue::Str(r.read_cstring()?),
            // Unknown type — try reading as int
            _ => ModificationValue::Int(r.read_s32()?),
        };

        let terminator = Rawcode::bin_read(r)?;

        Ok(Self {
            modification_id,
            value_type,
            level,
            data_index,
            value,
            terminator,
        })
    }
}

// ─── ModificationSet ─────────────────────────────────────────────────────────

/// A set of modifications (hexpat `struct ModificationSet`).
#[derive(Debug, Clone, Serialize)]
pub struct ModificationSet {
    /// Flags (only present in format ≥ 3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u32>,
    /// The modifications in this set.
    pub modifications: Vec<Modification>,
}

impl ModificationSet {
    fn read(r: &mut BinReader, format: u32, level_data: bool) -> BinResult<Self> {
        let flags = if format >= 3 { Some(r.read_u32()?) } else { None };
        let count = r.read_u32()?;
        let mut modifications = Vec::with_capacity(count as usize);
        for _ in 0..count {
            modifications.push(Modification::read(r, level_data)?);
        }
        Ok(Self { flags, modifications })
    }
}

// ─── ObjectDefinition ────────────────────────────────────────────────────────

/// A single object override (hexpat `struct ObjectDefinition`).
#[derive(Debug, Clone, Serialize)]
pub struct ObjectDefinition {
    /// Original (base) object rawcode.
    pub original_id: Rawcode,
    /// Custom (derived) object rawcode.  `"\0\0\0\0"` for non-custom objects.
    pub custom_id: Rawcode,
    /// Modification sets.
    pub sets: Vec<ModificationSet>,
}

impl ObjectDefinition {
    fn read(r: &mut BinReader, format: u32, level_data: bool) -> BinResult<Self> {
        let original_id = Rawcode::bin_read(r)?;
        let custom_id = Rawcode::bin_read(r)?;

        let sets = if format >= 3 {
            let count = r.read_u32()?;
            let mut v = Vec::with_capacity(count as usize);
            for _ in 0..count {
                v.push(ModificationSet::read(r, format, level_data)?);
            }
            v
        } else {
            vec![ModificationSet::read(r, format, level_data)?]
        };

        Ok(Self { original_id, custom_id, sets })
    }
}

// ─── ObjectTable ─────────────────────────────────────────────────────────────

/// Top-level container: original + custom object tables
/// (hexpat `struct ObjectTable`).
#[derive(Debug, Clone, Serialize)]
pub struct ObjectTable {
    /// Objects that modify standard (original) definitions.
    pub originals: Vec<ObjectDefinition>,
    /// Objects that are entirely custom (derived from an original).
    pub customs: Vec<ObjectDefinition>,
}

impl ObjectTable {
    fn read(r: &mut BinReader, format: u32, level_data: bool) -> BinResult<Self> {
        let original_count = r.read_u32()?;
        let mut originals = Vec::with_capacity(original_count as usize);
        for _ in 0..original_count {
            originals.push(ObjectDefinition::read(r, format, level_data)?);
        }

        let custom_count = r.read_u32()?;
        let mut customs = Vec::with_capacity(custom_count as usize);
        for _ in 0..custom_count {
            customs.push(ObjectDefinition::read(r, format, level_data)?);
        }

        Ok(Self { originals, customs })
    }
}

// ─── W3ObjectData (top-level) ────────────────────────────────────────────────

/// Parsed contents of a `.w3a / .w3b / .w3d / .w3h / .w3q / .w3t / .w3u` file.
#[derive(Debug, Clone, Serialize)]
pub struct W3ObjectData {
    /// Format version (first 4 bytes).
    pub format_version: u32,
    /// Whether this format uses level-based modifications (`.w3a`, `.w3d`, `.w3q`).
    pub level_data: bool,
    /// The object table.
    #[serde(flatten)]
    pub table: ObjectTable,
}

impl W3ObjectData {
    /// Parse an object-data file from raw bytes.
    ///
    /// * `level_data` — `true` for `.w3a` (abilities), `.w3d` (doodads),
    ///   `.w3q` (upgrades); `false` for `.w3b`, `.w3h`, `.w3t`, `.w3u`.
    pub fn read(data: &[u8], level_data: bool) -> BinResult<(Self, BinReaderMeta)> {
        let mut r = BinReader::new(data);

        let format_version = r.read_u32()?;
        let table = ObjectTable::read(&mut r, format_version, level_data)?;

        let meta = r.meta();
        Ok((W3ObjectData { format_version, level_data, table }, meta))
    }
}

