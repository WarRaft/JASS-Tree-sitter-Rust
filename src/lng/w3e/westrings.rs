//! Parser and cache for Warcraft III `WorldEditStrings.txt` files.
//!
//! These INI-like files map `WESTRING_*` keys to human-readable strings.
//! Multiple files can be layered — later loads override earlier entries.
//!
//! Format:
//! ```text
//! [WorldEditStrings]
//! WESTRING_DOOD_APMS=Mushrooms
//! WESTRING_RACE_HUMAN=Human
//! ```
//!
//! Access to individual entries is O(1) via `HashMap`.

use std::collections::HashMap;
use std::sync::Mutex;
use tree_sitter::Parser;
use crate::lng::bni::kind::Kind;

// ─── Parser ──────────────────────────────────────────────────────────────────

/// Parse a single `WorldEditStrings.txt` (or similar INI) buffer into
/// a `HashMap<String, String>` using the tree-sitter BNI grammar.
///
/// Section headers (`[…]`) are ignored.  For each `key=value` item the
/// value is extracted from the grammar nodes directly, so surrounding
/// quotes are never included (the grammar separates them at parse time).
pub fn parse_westrings(data: &[u8]) -> HashMap<String, String> {
    let text = String::from_utf8_lossy(data);
    // Strip BOM if present
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bni::LANGUAGE.into())
        .expect("Failed to set BNI language");

    let Some(tree) = parser.parse(text.as_bytes(), None) else {
        return HashMap::new();
    };

    let mut map = HashMap::new();
    let root = tree.root_node();

    let mut cursor = root.walk();
    for item_node in root.children(&mut cursor) {
        if Kind::try_from(item_node.grammar_id()) != Ok(Kind::Item) {
            continue;
        }

        // Extract key
        let mut key: Option<&str> = None;
        let mut value = String::new();

        let mut child_cursor = item_node.walk();
        for child in item_node.children(&mut child_cursor) {
            let Ok(kind) = Kind::try_from(child.grammar_id()) else {
                continue;
            };
            match kind {
                Kind::Key => {
                    key = child.utf8_text(text.as_bytes()).ok();
                }
                Kind::ValueList => {
                    // Walk the value_list children and collect the first
                    // meaningful value (string content, unquoted, int, float).
                    let mut val_cursor = child.walk();
                    for val_child in child.children(&mut val_cursor) {
                        let Ok(vk) = Kind::try_from(val_child.grammar_id()) else {
                            continue;
                        };
                        match vk {
                            Kind::QuotedString => {
                                // Descend into quoted_string to get string_content
                                let mut qs_cursor = val_child.walk();
                                for qs_child in val_child.children(&mut qs_cursor) {
                                    let Ok(qk) = Kind::try_from(qs_child.grammar_id()) else {
                                        continue;
                                    };
                                    if matches!(qk, Kind::DqStringContent | Kind::SqStringContent) {
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
                map.insert(k.to_string(), value);
            }
        }
    }

    map
}

// ─── Global cache ────────────────────────────────────────────────────────────

static WESTRINGS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Load `UI\WorldEditStrings.txt` via the cascading file lookup and cache it.
///
/// If already loaded, returns immediately.  Call [`invalidate`] to force a
/// reload (e.g. when the game path changes).
pub fn ensure_loaded(archive_path: Option<&str>) {
    let mut guard = match WESTRINGS.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    if guard.is_some() {
        return;
    }

    let mut map = HashMap::new();

    // Load from cascading lookup (picks highest-priority source).
    if let Some((buf, _source)) =
        super::file_lookup::lookup_file("UI\\WorldEditStrings.txt", archive_path)
    {
        map = parse_westrings(&buf);
    }

    *guard = Some(map);
}

/// Look up a single key (e.g. `"WESTRING_DOOD_APMS"`) → `Some("Mushrooms")`.
///
/// Returns `None` if the cache is empty or the key is absent.
pub fn resolve(key: &str) -> Option<String> {
    let guard = match WESTRINGS.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    guard.as_ref()?.get(key).cloned()
}

/// Resolve a value that may itself be a `WESTRING_*` reference (one level).
///
/// If `value` starts with `"WESTRING_"`, try to look it up; otherwise return
/// the value unchanged.  Also handles the rare case where a resolved value
/// is itself a `WESTRING_*` reference (up to 3 levels).
pub fn resolve_value(value: &str) -> String {
    let mut current = value.to_string();
    for _ in 0..3 {
        if !current.starts_with("WESTRING_") {
            break;
        }
        match resolve(&current) {
            Some(resolved) => current = resolved,
            None => break,
        }
    }
    current
}

/// Drop the cached map so the next [`ensure_loaded`] call re-reads the file.
pub fn invalidate() {
    let mut guard = match WESTRINGS.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    *guard = None;
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_parse() {
        let data = b"[WorldEditStrings]\nWESTRING_FOO=Bar\nWESTRING_BAZ=Hello World\n";
        let map = parse_westrings(data);
        assert_eq!(map.get("WESTRING_FOO").map(|s| s.as_str()), Some("Bar"));
        assert_eq!(
            map.get("WESTRING_BAZ").map(|s| s.as_str()),
            Some("Hello World")
        );
    }

    #[test]
    fn skip_comments_and_sections() {
        let data = b"// comment\n[Section]\nKEY=val\n//another\n";
        let map = parse_westrings(data);
        assert_eq!(map.get("KEY").map(|s| s.as_str()), Some("val"));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn bom_handling() {
        let data = b"\xEF\xBB\xBF[WorldEditStrings]\nWESTRING_X=Y\n";
        let map = parse_westrings(data);
        assert_eq!(map.get("WESTRING_X").map(|s| s.as_str()), Some("Y"));
    }

    #[test]
    fn strip_surrounding_quotes() {
        let data = b"[WorldEditStrings]\nWESTRING_Q=\"City Building (Diagonal 1, Red)\"\nWESTRING_P=Plain\n";
        let map = parse_westrings(data);
        assert_eq!(map.get("WESTRING_Q").map(|s| s.as_str()), Some("City Building (Diagonal 1, Red)"));
        assert_eq!(map.get("WESTRING_P").map(|s| s.as_str()), Some("Plain"));
    }

    #[test]
    fn resolve_chain() {
        // Reset cache
        invalidate();
        {
            let mut guard = WESTRINGS.lock().unwrap();
            let mut map = HashMap::new();
            map.insert("WESTRING_A".into(), "WESTRING_B".into());
            map.insert("WESTRING_B".into(), "Final Value".into());
            *guard = Some(map);
        }
        assert_eq!(resolve_value("WESTRING_A"), "Final Value");
        assert_eq!(resolve_value("plain text"), "plain text");
        // Cleanup
        invalidate();
    }
}

