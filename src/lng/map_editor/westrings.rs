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

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use tree_sitter::Parser;
use crate::lng::bni::kind::Kind;

// ─── GameString ──────────────────────────────────────────────────────────────

/// A string value that may have been resolved from a `WESTRING_*` reference.
///
/// When serialized to JSON:
/// - If no WESTRING resolution occurred (`original == value`), emits a plain string.
/// - Otherwise emits `{"value": "...", "original": "...", "source": "..."}`.
#[derive(Debug, Clone)]
pub struct GameString {
    /// The resolved (display) value.
    pub value: String,
    /// The original raw value (e.g. `"WESTRING_GE_BRIDGE"`).
    /// Equal to `value` when no resolution occurred.
    pub original: String,
    /// Source file that provided the resolution (e.g. `"WorldEditStrings.txt"`).
    /// Empty when no resolution occurred.
    pub source: String,
}

impl GameString {
    /// Create a plain (non-resolved) GameString.
    pub fn plain(value: String) -> Self {
        Self {
            original: value.clone(),
            value,
            source: String::new(),
        }
    }

    /// Whether a WESTRING resolution was applied.
    pub fn is_resolved(&self) -> bool {
        !self.source.is_empty() && self.original != self.value
    }
}

impl From<String> for GameString {
    fn from(s: String) -> Self {
        Self::plain(s)
    }
}

impl Serialize for GameString {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.is_resolved() {
            use serde::ser::SerializeStruct;
            let mut s = serializer.serialize_struct("GameString", 3)?;
            s.serialize_field("value", &self.value)?;
            s.serialize_field("original", &self.original)?;
            s.serialize_field("source", &self.source)?;
            s.end()
        } else {
            serializer.serialize_str(&self.value)
        }
    }
}

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
                                    if matches!(qk, Kind::StringContent) {
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

/// Each entry stores `(resolved_value, source_file)`.
pub(crate) static WESTRINGS: Mutex<Option<HashMap<String, (String, String)>>> = Mutex::new(None);

/// Files to load, in priority order (later files override earlier entries).
const STRING_FILES: &[&str] = &[
    "UI\\WorldEditStrings.txt",
    "UI\\WorldEditGameStrings.txt",
];

/// Load all WorldEdit string files via cascading file lookup and cache them.
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

    let mut map: HashMap<String, (String, String)> = HashMap::new();

    for &file_path in STRING_FILES {
        if let Some((buf, _lookup_source)) =
            super::file_lookup::lookup_file(file_path, archive_path)
        {
            // Extract just the filename for display (e.g. "WorldEditStrings.txt")
            let source_name = file_path
                .rsplit('\\')
                .next()
                .unwrap_or(file_path)
                .to_string();

            let parsed = parse_westrings(&buf);
            for (k, v) in parsed {
                map.insert(k, (v, source_name.clone()));
            }
        }
    }

    *guard = Some(map);
}

/// Look up a single key (e.g. `"WESTRING_DOOD_APMS"`) → `Some(("Mushrooms", "WorldEditStrings.txt"))`.
///
/// Returns `None` if the cache is empty or the key is absent.
pub fn resolve(key: &str) -> Option<(String, String)> {
    let guard = match WESTRINGS.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    guard.as_ref()?.get(key).cloned()
}

/// Convenience wrapper: resolve a `WESTRING_*` key (or return the input
/// unchanged) and return only the display string.
#[allow(dead_code)]
pub fn resolve_value(raw_value: &str) -> String {
    resolve_game_string(raw_value).value
}

/// Resolve a value into a [`GameString`] that tracks provenance.
///
/// If the value starts with `"WESTRING_"`, looks it up and returns a
/// `GameString` with `original`, `value` (resolved), and `source` populated.
/// Otherwise returns a plain `GameString` (`original == value`, empty `source`).
pub fn resolve_game_string(raw_value: &str) -> GameString {
    if raw_value.is_empty() {
        return GameString::plain(String::new());
    }

    let original = raw_value.to_string();
    let mut current = raw_value.to_string();
    let mut source = String::new();

    for _ in 0..3 {
        if !current.starts_with("WESTRING_") {
            break;
        }
        match resolve(&current) {
            Some((resolved, src)) => {
                source = src;
                current = resolved;
            }
            None => break,
        }
    }

    GameString {
        value: current,
        original,
        source,
    }
}

/// Drop the cached map so the next [`ensure_loaded`] call re-reads the file.
pub fn invalidate() {
    let mut guard = match WESTRINGS.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    *guard = None;
}

/// Return a clone of the full WESTRING map (keys → resolved values).
pub fn get_all() -> HashMap<String, String> {
    let guard = match WESTRINGS.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    match guard.as_ref() {
        Some(m) => m.iter().map(|(k, (v, _))| (k.clone(), v.clone())).collect(),
        None => HashMap::new(),
    }
}

