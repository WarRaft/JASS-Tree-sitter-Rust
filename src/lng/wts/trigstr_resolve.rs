//! Lightweight WTS parser for TRIGSTR resolution.
//!
//! Parses `war3map.wts` bytes into a `HashMap<String, String>` (id → text)
//! and provides helpers to resolve `TRIGSTR_<id>` references in strings
//! and JSON values.

use std::collections::HashMap;

/// Parse a `war3map.wts` file into a map of identifier → string content.
///
/// WTS format:
/// ```text
/// STRING 000
/// // optional comment
/// {
/// line 1
/// line 2
/// }
/// ```
///
/// Keys are normalised to their canonical integer form (`"000"` → `"0"`,
/// `"011"` → `"11"`) so that look-ups from `TRIGSTR_011` and `TRIGSTR_11`
/// both resolve correctly regardless of zero-padding style.
pub fn parse_wts_strings(data: &[u8]) -> HashMap<String, String> {
    parse_wts_strings_with_lines(data)
        .into_iter()
        .map(|(k, (v, _line))| (k, v))
        .collect()
}

/// Like [`parse_wts_strings`] but also returns the 0-based line number of
/// each `STRING <id>` header.  Used by the map-string cache to enable
/// click-to-navigate from resolved `TRIGSTR_*` names in the UI.
pub fn parse_wts_strings_with_lines(data: &[u8]) -> HashMap<String, (String, usize)> {
    let text = String::from_utf8_lossy(data);
    // Strip leading UTF-8 BOM if present (\u{FEFF})
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
    let mut map = HashMap::new();

    let all_lines: Vec<&str> = text.lines().collect();
    let mut idx = 0;

    while idx < all_lines.len() {
        let trimmed = all_lines[idx].trim();

        // Look for "STRING <id>"
        if !trimmed.starts_with("STRING ") {
            idx += 1;
            continue;
        }

        let header_line = idx;
        let raw_id = trimmed["STRING ".len()..].trim();
        if raw_id.is_empty() {
            idx += 1;
            continue;
        }

        // Normalise numeric IDs: "000" → "0", "011" → "11"
        let id = match raw_id.parse::<u32>() {
            Ok(n) => n.to_string(),
            Err(_) => raw_id.to_string(),
        };

        idx += 1;

        // Skip comment lines and blank lines until we find '{'
        loop {
            if idx >= all_lines.len() { break; }
            let t = all_lines[idx].trim();
            if t == "{" {
                idx += 1; // consume '{'
                break;
            }
            if t.starts_with("//") || t.is_empty() {
                idx += 1; // skip comment / blank
                continue;
            }
            // Unexpected non-comment, non-brace line — give up on this entry
            break;
        }

        // Collect lines until '}'
        let mut body = Vec::new();
        while idx < all_lines.len() {
            if all_lines[idx].trim() == "}" {
                idx += 1;
                break;
            }
            body.push(all_lines[idx]);
            idx += 1;
        }

        let value = body.join("\n");
        map.insert(id, (value, header_line));
    }

    map
}

/// If `s` is exactly `TRIGSTR_<id>`, look up `<id>` in `wts` and return
/// the resolved text.  Otherwise return the original string unchanged.
///
/// The look-up normalises numeric IDs (`"011"` → `"11"`) to match the
/// canonical keys produced by [`parse_wts_strings`].
pub fn resolve_trigstr(s: &str, wts: &HashMap<String, String>) -> String {
    if let Some(raw_id) = s.strip_prefix("TRIGSTR_") {
        let key = match raw_id.parse::<u32>() {
            Ok(n) => n.to_string(),
            Err(_) => raw_id.to_string(),
        };
        if let Some(value) = wts.get(&key) {
            return value.clone();
        }
    }
    s.to_string()
}

/// Recursively walk a JSON value and resolve every string that looks like
/// `TRIGSTR_<id>` using the provided WTS map.
pub fn resolve_trigstr_json(
    val: &mut serde_json::Value,
    wts: &HashMap<String, String>,
) {
    match val {
        serde_json::Value::String(s) => {
            if s.starts_with("TRIGSTR_") {
                *s = resolve_trigstr(s, wts);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                resolve_trigstr_json(item, wts);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                resolve_trigstr_json(v, wts);
            }
        }
        _ => {}
    }
}

