//! Generic SYLK (`.slk`) parser, shared types and per-domain loaders.
//!
//! Domain modules:
//! - [`terrain`] — `TerrainArt\Terrain.slk`
//! - [`cliff`]   — `TerrainArt\CliffTypes.slk` + cliff variations
//! - [`water`]   — `TerrainArt\Water.slk`
//! - [`doodad`]  — `Doodads\Doodads.slk` + `war3map.w3d` merge
//! - [`unit`]    — `Units\UnitData.slk` (+ Balance, UI, Weapons) + `war3map.w3u`
//! - [`destructable`] — `Units\DestructableData.slk` + `war3map.w3b`

pub mod terrain;
pub mod cliff;
pub mod water;
pub mod doodad;
pub mod unit;
pub mod destructable;

// Re-export everything so existing `use crate::lng::map_editor::slk::*` keeps working.
pub use terrain::*;
pub use cliff::*;
pub use water::*;
pub use doodad::*;
pub use unit::*;
pub use destructable::*;

use serde::Serialize;
use std::collections::HashMap;
use tree_sitter::Parser;
use crate::lng::bni::kind::Kind;

// ─── Color ───────────────────────────────────────────────────────────────────

/// RGBA colour.
#[derive(Debug, Clone, Serialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

// ─── Generic SLK parser ──────────────────────────────────────────────────────

/// Parse a SYLK file into a list of row maps.
///
/// Row 1 is treated as headers; every subsequent row becomes a
/// `HashMap<header, value>`.  Returns an empty vec on malformed input.
pub fn parse_slk(data: &[u8]) -> Vec<HashMap<String, String>> {
    let text = String::from_utf8_lossy(data);

    let mut cols: usize = 0;
    let mut rows: usize = 0;
    let mut headers: Vec<String> = Vec::new();
    let mut result: Vec<HashMap<String, String>> = Vec::new();

    // Sticky coordinates (SYLK carries forward the last X / Y seen).
    let mut cur_x: usize = 1;
    let mut cur_y: usize = 1;

    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }

        // ── B record: dimensions ─────────────────────────────────
        if line.starts_with("B;") {
            for part in line[2..].split(';') {
                if let Some(v) = part.strip_prefix('X') {
                    cols = v.parse().unwrap_or(0);
                } else if let Some(v) = part.strip_prefix('Y') {
                    rows = v.parse().unwrap_or(0);
                }
            }
            if rows > 1 {
                result.reserve(rows - 1);
            }
            if cols > 0 {
                headers.resize(cols, String::new());
            }
            continue;
        }

        // ── C record: cell value ─────────────────────────────────
        if line.starts_with("C;") {
            let mut x: Option<usize> = None;
            let mut y: Option<usize> = None;
            let mut k_value: Option<&str> = None;

            for part in line[2..].split(';') {
                if let Some(v) = part.strip_prefix('X') {
                    x = v.parse().ok();
                } else if let Some(v) = part.strip_prefix('Y') {
                    y = v.parse().ok();
                } else if let Some(v) = part.strip_prefix('K') {
                    k_value = Some(v);
                }
            }

            if let Some(yy) = y {
                cur_y = yy;
            }
            if let Some(xx) = x {
                cur_x = xx;
            }

            let Some(raw_k) = k_value else { continue };

            // Strip surrounding quotes from string values.
            let value = if raw_k.starts_with('"') && raw_k.ends_with('"') && raw_k.len() >= 2 {
                &raw_k[1..raw_k.len() - 1]
            } else {
                raw_k
            };

            let ci = cur_x.saturating_sub(1); // 0-based column index

            if cur_y == 1 {
                // Header row
                if ci < headers.len() {
                    headers[ci] = value.to_string();
                } else {
                    // SLK without a B record, or extra columns
                    if headers.len() <= ci {
                        headers.resize(ci + 1, String::new());
                    }
                    headers[ci] = value.to_string();
                }
            } else {
                // Data row (1-based row 2 → result index 0)
                let ri = cur_y - 2;
                if result.len() <= ri {
                    result.resize_with(ri + 1, HashMap::new);
                }
                if ci < headers.len() && !headers[ci].is_empty() {
                    result[ri].insert(headers[ci].clone(), value.to_string());
                }
            }

            continue;
        }

        // All other records (ID, F, E, …) are ignored.
    }

    result
}

// ─── INI-style UnitStrings parser ─────────────────────────────────────────────

/// Parse an INI-format UnitStrings.txt file into a map of section rawcodes to
/// field key/value maps.
///
/// Format:
/// ```text
/// [Hamg]
/// Name=Archmage
/// Tip=Summon |cffffcc00A|rrchmage
/// Ubertip="Mystical Hero, adept at ranged assaults..."
/// ```
///
/// Returns `HashMap<"Hamg", {"Name": "Archmage", "Tip": "Summon ...", ...}>`.
pub fn parse_unit_strings(data: &[u8]) -> HashMap<String, HashMap<String, String>> {
    let text = String::from_utf8_lossy(data);
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bni::LANGUAGE.into())
        .expect("Failed to set BNI language");

    let Some(tree) = parser.parse(text.as_bytes(), None) else {
        return HashMap::new();
    };

    let mut result: HashMap<String, HashMap<String, String>> = HashMap::new();
    let root = tree.root_node();
    let mut current_section: Option<String> = None;

    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        let Ok(kind) = Kind::try_from(node.grammar_id()) else {
            continue;
        };
        match kind {
            Kind::Section => {
                // Extract section name from children
                let mut sc = node.walk();
                for child in node.children(&mut sc) {
                    if Kind::try_from(child.grammar_id()) == Ok(Kind::SectionName) {
                        if let Ok(name) = child.utf8_text(text.as_bytes()) {
                            current_section = Some(name.to_string());
                            result.entry(name.to_string()).or_default();
                        }
                        break;
                    }
                }
            }
            Kind::Item => {
                let Some(ref section) = current_section else { continue };

                let mut key: Option<&str> = None;
                let mut value = String::new();

                let mut child_cursor = node.walk();
                for child in node.children(&mut child_cursor) {
                    let Ok(ck) = Kind::try_from(child.grammar_id()) else {
                        continue;
                    };
                    match ck {
                        Kind::Key => {
                            key = child.utf8_text(text.as_bytes()).ok();
                        }
                        Kind::ValueList => {
                            let mut val_cursor = child.walk();
                            for val_child in child.children(&mut val_cursor) {
                                let Ok(vk) = Kind::try_from(val_child.grammar_id()) else {
                                    continue;
                                };
                                match vk {
                                    Kind::QuotedString => {
                                        let mut qs_cursor = val_child.walk();
                                        for qs_child in val_child.children(&mut qs_cursor) {
                                            if Kind::try_from(qs_child.grammar_id())
                                                == Ok(Kind::StringContent)
                                            {
                                                value = qs_child
                                                    .utf8_text(text.as_bytes())
                                                    .unwrap_or_default()
                                                    .to_string();
                                                break;
                                            }
                                        }
                                        break;
                                    }
                                    Kind::UnquotedString | Kind::Int | Kind::Float => {
                                        value = val_child
                                            .utf8_text(text.as_bytes())
                                            .unwrap_or_default()
                                            .to_string();
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }

                if let Some(k) = key {
                    if !k.is_empty() {
                        result.get_mut(section).unwrap().insert(k.to_string(), value);
                    }
                }
            }
            _ => {}
        }
    }

    result
}

// ─── Shared SLK field helpers ────────────────────────────────────────────────

/// Helper: parse an SLK field as `u8`, defaulting to `def`.
pub fn slk_u8(row: &HashMap<String, String>, key: &str, def: u8) -> u8 {
    row.get(key).and_then(|v| v.parse().ok()).unwrap_or(def)
}

/// Helper: parse an SLK field as `u32`, defaulting to `def`.
pub fn slk_u32(row: &HashMap<String, String>, key: &str, def: u32) -> u32 {
    row.get(key).and_then(|v| v.parse().ok()).unwrap_or(def)
}

/// Helper: parse an SLK field as `f64`, defaulting to `def`.
pub fn slk_f64(row: &HashMap<String, String>, key: &str, def: f64) -> f64 {
    row.get(key).and_then(|v| v.parse().ok()).unwrap_or(def)
}

/// Helper: parse an SLK field as boolean (`"1"` = true).
pub fn slk_bool(row: &HashMap<String, String>, key: &str) -> bool {
    row.get(key).map(|v| v == "1").unwrap_or(false)
}

/// Helper: read an SLK string, returning empty for `"_"`, `"-"`, or missing.
pub fn slk_str(row: &HashMap<String, String>, key: &str) -> String {
    row.get(key)
        .filter(|v| *v != "_" && *v != "-")
        .cloned()
        .unwrap_or_default()
}

/// Convert a 4-char SLK rawcode string to its `u32` key (little-endian).
pub fn rawcode_to_u32(id: &str) -> u32 {
    let bytes = id.as_bytes();
    let mut b = [0u8; 4];
    for (i, &byte) in bytes.iter().take(4).enumerate() {
        b[i] = byte;
    }
    u32::from_le_bytes(b)
}

/// Helper: build a `HashMap<String, HashMap<String,String>>` from SLK rows,
/// keyed by the given ID column.
pub fn slk_index_by(rows: Vec<HashMap<String, String>>, id_col: &str) -> HashMap<String, HashMap<String, String>> {
    let mut map = HashMap::new();
    for row in rows {
        if let Some(id) = row.get(id_col).filter(|v| !v.is_empty()) {
            map.insert(id.clone(), row);
        }
    }
    map
}

// ─── Modification value helpers (shared by w3d / w3b / w3u / … merge code) ──

use crate::lng::w3abdhqtu::parse::ModificationValue;

/// Extract a string from a modification value.
pub fn mod_value_string(v: &ModificationValue) -> Option<String> {
    match v {
        ModificationValue::Str(s) => Some(s.clone()),
        ModificationValue::Int(i) => Some(i.to_string()),
        ModificationValue::Real(f) | ModificationValue::Unreal(f) => Some(f.to_string()),
    }
}

/// Extract a u32 from a modification value.
pub fn mod_value_u32(v: &ModificationValue) -> Option<u32> {
    match v {
        ModificationValue::Int(i) => Some(*i as u32),
        ModificationValue::Real(f) | ModificationValue::Unreal(f) => Some(*f as u32),
        ModificationValue::Str(s) => s.parse().ok(),
    }
}

/// Extract an f64 from a modification value.
pub fn mod_value_f64(v: &ModificationValue) -> Option<f64> {
    match v {
        ModificationValue::Int(i) => Some(*i as f64),
        ModificationValue::Real(f) | ModificationValue::Unreal(f) => Some(*f as f64),
        ModificationValue::Str(s) => s.parse().ok(),
    }
}

/// Extract a bool from a modification value.
pub fn mod_value_bool(v: &ModificationValue) -> Option<bool> {
    match v {
        ModificationValue::Int(i) => Some(*i != 0),
        ModificationValue::Real(f) | ModificationValue::Unreal(f) => Some(*f != 0.0),
        ModificationValue::Str(s) => {
            let s = s.trim().to_lowercase();
            Some(s == "1" || s == "true" || s == "yes")
        }
    }
}

// ─── Per-SLK source info (shared by unit and potentially other multi-SLK loaders) ──

/// Per-SLK source info.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlkSource {
    /// SLK file name, e.g. `"UnitData.slk"`.
    pub name: String,
    /// Where the file was found, e.g. `"War3Patch.mpq"`.
    pub source: String,
    /// Number of rows parsed.
    pub rows: usize,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod slk_test;
#[cfg(test)]
mod cliff_test;
#[cfg(test)]
mod doodad_test;
#[cfg(test)]
mod unit_test;
#[cfg(test)]
mod destructable_test;
