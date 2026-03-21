//! Shared string-literal semantic tokenization and color extraction.
//!
//! Used by both JASS and AS cursor passes.  Handles:
//! - Standard escape sequences (`\\`, `\"`, `\n`, `\t`, `\r`, `\0`)
//! - Warcraft color codes: `|cAARRGGBB`, `|r`, `|n` (case-insensitive)
//! - Color picker info for `|cAARRGGBB` inside strings
//!   and `0xAARRGGBB` / `0xRRGGBB` hex literals.

use crate::lsp::color::lsp::{Color, ColorInformation};
use crate::lsp::range::Range;
use crate::lsp::semantic::hub::Hub;
use crate::lsp::semantic::lsp::Kind as TokenKind;
use lapce_xi_rope::Rope;
use tree_sitter::Node;

// ─── Semantic tokenization of string contents ────────────────────────────────

/// Tokenize the interior of a string literal node, emitting sub-range
/// semantic tokens: `String` for plain text, `Keyword` for escape sequences
/// and Warcraft `|c…` / `|r` / `|n` codes.
///
/// The node is expected to include the surrounding quotes.
pub fn tokenize_string_literal(node: &Node, rope: &Rope, hub: &mut Hub) {
    let sb = node.start_byte();
    let eb = node.end_byte();
    if eb <= sb + 1 {
        // Empty or single-char (just a quote) — mark whole node as String
        hub.add_range(sb, eb - sb, rope, TokenKind::String, 0u32);
        return;
    }

    let text = rope.slice_to_cow(sb..eb);
    let bytes = text.as_bytes();

    // Opening quote
    hub.add_range(sb, 1, rope, TokenKind::String, 0u32);

    // Inner content (between quotes)
    let inner_start = 1usize;
    let inner_end = if bytes.len() >= 2
        && (bytes[bytes.len() - 1] == b'"' || bytes[bytes.len() - 1] == b'\'')
    {
        bytes.len() - 1
    } else {
        bytes.len()
    };

    let mut i = inner_start;
    let mut plain_start = i;

    while i < inner_end {
        let b = bytes[i];

        // ── Backslash escapes (\n, \t, \\, \", etc.) ─────────────────────
        if b == b'\\' && i + 1 < inner_end {
            // Flush preceding plain text as String
            if i > plain_start {
                hub.add_range(sb + plain_start, i - plain_start, rope, TokenKind::String, 0u32);
            }
            // Emit escape as Keyword (2 bytes: \ + char)
            hub.add_range(sb + i, 2, rope, TokenKind::Keyword, 0u32);
            i += 2;
            plain_start = i;
            continue;
        }

        // ── Warcraft pipe codes (|cAARRGGBB, |r, |n) ────────────────────
        if b == b'|' && i + 1 < inner_end {
            let next = bytes[i + 1];
            // |c or |C followed by 8 hex digits
            if (next == b'c' || next == b'C') && i + 10 <= inner_end {
                let hex_slice = &bytes[i + 2..i + 10];
                if hex_slice.iter().all(|b| b.is_ascii_hexdigit()) {
                    if i > plain_start {
                        hub.add_range(sb + plain_start, i - plain_start, rope, TokenKind::String, 0u32);
                    }
                    hub.add_range(sb + i, 10, rope, TokenKind::Keyword, 0u32);
                    i += 10;
                    plain_start = i;
                    continue;
                }
            }
            // |r or |R, |n or |N
            if next == b'r' || next == b'R' || next == b'n' || next == b'N' {
                if i > plain_start {
                    hub.add_range(sb + plain_start, i - plain_start, rope, TokenKind::String, 0u32);
                }
                hub.add_range(sb + i, 2, rope, TokenKind::Keyword, 0u32);
                i += 2;
                plain_start = i;
                continue;
            }
        }

        i += 1;
    }

    // Flush remaining plain text
    if inner_end > plain_start {
        hub.add_range(sb + plain_start, inner_end - plain_start, rope, TokenKind::String, 0u32);
    }

    // Closing quote (if present)
    if inner_end < bytes.len() {
        hub.add_range(sb + inner_end, 1, rope, TokenKind::String, 0u32);
    }
}

/// Tokenize an **unquoted** string node (e.g. WTS `string_text`), emitting
/// `String` for plain text and `Keyword` for `|cAARRGGBB` / `|r` / `|n`.
///
/// Unlike [`tokenize_string_literal`], the node is NOT expected to have
/// surrounding quotes — the entire byte range is treated as content.
pub fn tokenize_raw_string(node: &Node, rope: &Rope, hub: &mut Hub) {
    let sb = node.start_byte();
    let eb = node.end_byte();
    if eb <= sb {
        return;
    }

    let text = rope.slice_to_cow(sb..eb);
    let bytes = text.as_bytes();
    let len = bytes.len();

    let mut i = 0usize;
    let mut plain_start = 0usize;

    while i < len {
        let b = bytes[i];

        // ── Warcraft pipe codes (|cAARRGGBB, |r, |n) ────────────────────
        if b == b'|' && i + 1 < len {
            let next = bytes[i + 1];
            // |c or |C followed by 8 hex digits
            if (next == b'c' || next == b'C') && i + 10 <= len {
                let hex_slice = &bytes[i + 2..i + 10];
                if hex_slice.iter().all(|b| b.is_ascii_hexdigit()) {
                    if i > plain_start {
                        hub.add_range(sb + plain_start, i - plain_start, rope, TokenKind::String, 0u32);
                    }
                    hub.add_range(sb + i, 10, rope, TokenKind::Keyword, 0u32);
                    i += 10;
                    plain_start = i;
                    continue;
                }
            }
            // |r or |R, |n or |N
            if next == b'r' || next == b'R' || next == b'n' || next == b'N' {
                if i > plain_start {
                    hub.add_range(sb + plain_start, i - plain_start, rope, TokenKind::String, 0u32);
                }
                hub.add_range(sb + i, 2, rope, TokenKind::Keyword, 0u32);
                i += 2;
                plain_start = i;
                continue;
            }
        }

        i += 1;
    }

    // Flush remaining plain text
    if len > plain_start {
        hub.add_range(sb + plain_start, len - plain_start, rope, TokenKind::String, 0u32);
    }
}

// ─── Color extraction ────────────────────────────────────────────────────────

/// Collect color information from a string literal node (`|cAARRGGBB`).
pub fn collect_string_colors(node: &Node, rope: &Rope) -> Vec<ColorInformation> {
    let sb = node.start_byte();
    let eb = node.end_byte();
    if eb <= sb + 1 {
        return vec![];
    }

    let text = rope.slice_to_cow(sb..eb);
    let bytes = text.as_bytes();
    let mut colors = Vec::new();

    let inner_end = if bytes.len() >= 2
        && (bytes[bytes.len() - 1] == b'"' || bytes[bytes.len() - 1] == b'\'')
    {
        bytes.len() - 1
    } else {
        bytes.len()
    };

    let mut i = 1usize; // skip opening quote
    while i < inner_end {
        let b = bytes[i];

        // Skip backslash escapes
        if b == b'\\' && i + 1 < inner_end {
            i += 2;
            continue;
        }

        // |cAARRGGBB
        if b == b'|' && i + 1 < inner_end {
            let next = bytes[i + 1];
            if (next == b'c' || next == b'C') && i + 10 <= inner_end {
                let hex_slice = &bytes[i + 2..i + 10];
                if hex_slice.iter().all(|b| b.is_ascii_hexdigit()) {
                    if let Some(color) = parse_aarrggbb(hex_slice) {
                        let range = Range::from_byte_offsets(rope, sb + i, sb + i + 10);
                        colors.push(ColorInformation { range, color });
                    }
                    i += 10;
                    continue;
                }
            }
            // |r, |n — skip
            if next == b'r' || next == b'R' || next == b'n' || next == b'N' {
                i += 2;
                continue;
            }
        }

        i += 1;
    }

    colors
}

/// Collect color information from an **unquoted** string node (WTS `string_text`).
///
/// Same as [`collect_string_colors`] but does not skip surrounding quotes.
pub fn collect_raw_string_colors(node: &Node, rope: &Rope) -> Vec<ColorInformation> {
    let sb = node.start_byte();
    let eb = node.end_byte();
    if eb <= sb {
        return vec![];
    }

    let text = rope.slice_to_cow(sb..eb);
    let bytes = text.as_bytes();
    let mut colors = Vec::new();
    let len = bytes.len();

    let mut i = 0usize;
    while i < len {
        let b = bytes[i];

        // |cAARRGGBB
        if b == b'|' && i + 1 < len {
            let next = bytes[i + 1];
            if (next == b'c' || next == b'C') && i + 10 <= len {
                let hex_slice = &bytes[i + 2..i + 10];
                if hex_slice.iter().all(|b| b.is_ascii_hexdigit()) {
                    if let Some(color) = parse_aarrggbb(hex_slice) {
                        let range = Range::from_byte_offsets(rope, sb + i, sb + i + 10);
                        colors.push(ColorInformation { range, color });
                    }
                    i += 10;
                    continue;
                }
            }
            // |r, |n — skip
            if next == b'r' || next == b'R' || next == b'n' || next == b'N' {
                i += 2;
                continue;
            }
        }

        i += 1;
    }

    colors
}

/// Collect color information from a hex literal node (`0xAARRGGBB` or `0xRRGGBB`).
pub fn collect_hex_literal_color(node: &Node, rope: &Rope) -> Option<ColorInformation> {
    let sb = node.start_byte();
    let eb = node.end_byte();
    let text = rope.slice_to_cow(sb..eb);
    let text = text.as_ref();

    // Must start with 0x or 0X
    if !text.starts_with("0x") && !text.starts_with("0X") {
        return None;
    }

    let hex = &text[2..];
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }

    match hex.len() {
        8 => {
            // 0xAARRGGBB
            let color = parse_aarrggbb(hex.as_bytes())?;
            let range = Range::from_byte_offsets(rope, sb, eb);
            Some(ColorInformation { range, color })
        }
        6 => {
            // 0xRRGGBB (alpha = FF)
            let color = parse_rrggbb(hex.as_bytes())?;
            let range = Range::from_byte_offsets(rope, sb, eb);
            Some(ColorInformation { range, color })
        }
        _ => None,
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn hex_byte(hi: u8, lo: u8) -> Option<u8> {
    Some(hex_digit(hi)? * 16 + hex_digit(lo)?)
}

/// Parse `AARRGGBB` (8 hex chars) into an LSP Color (RGBA, 0.0–1.0).
fn parse_aarrggbb(hex: &[u8]) -> Option<Color> {
    if hex.len() != 8 {
        return None;
    }
    let a = hex_byte(hex[0], hex[1])?;
    let r = hex_byte(hex[2], hex[3])?;
    let g = hex_byte(hex[4], hex[5])?;
    let b = hex_byte(hex[6], hex[7])?;
    Some(Color {
        red: r as f64 / 255.0,
        green: g as f64 / 255.0,
        blue: b as f64 / 255.0,
        alpha: a as f64 / 255.0,
    })
}

/// Parse `RRGGBB` (6 hex chars) into an LSP Color with alpha=1.0.
fn parse_rrggbb(hex: &[u8]) -> Option<Color> {
    if hex.len() != 6 {
        return None;
    }
    let r = hex_byte(hex[0], hex[1])?;
    let g = hex_byte(hex[2], hex[3])?;
    let b = hex_byte(hex[4], hex[5])?;
    Some(Color {
        red: r as f64 / 255.0,
        green: g as f64 / 255.0,
        blue: b as f64 / 255.0,
        alpha: 1.0,
    })
}

/// Format a Color as `|cAARRGGBB` string (for presentation inside strings).
pub fn color_to_pipe_string(color: &Color) -> String {
    let a = (color.alpha * 255.0).round() as u8;
    let r = (color.red * 255.0).round() as u8;
    let g = (color.green * 255.0).round() as u8;
    let b = (color.blue * 255.0).round() as u8;
    format!("|c{:02X}{:02X}{:02X}{:02X}", a, r, g, b)
}

/// Format a Color as `0xAARRGGBB` string (for hex literal presentation).
pub fn color_to_hex_string(color: &Color) -> String {
    let a = (color.alpha * 255.0).round() as u8;
    let r = (color.red * 255.0).round() as u8;
    let g = (color.green * 255.0).round() as u8;
    let b = (color.blue * 255.0).round() as u8;
    format!("0x{:02X}{:02X}{:02X}{:02X}", a, r, g, b)
}

