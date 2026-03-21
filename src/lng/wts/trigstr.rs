//! Global TRIGSTR map: WTS identifier → definition location.
//!
//! Populated during WTS parse.  Each `STRING <id> { … }` block stores
//! the identifier text (e.g. `"000"`) as key and the header-node range
//! as value.  Other files referencing `TRIGSTR_000` can look up the
//! definition here for go-to-definition / hover.
//!
//! The map is keyed by **WTS file URI** → `HashMap<String, TrigstrEntry>`,
//! so multiple WTS files can coexist without collisions and removing a
//! file from the map is O(1).

use crate::lsp::range::Range;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use url::Url;

/// A single TRIGSTR definition.
#[derive(Debug, Clone)]
pub struct TrigstrEntry {
    /// Range of the `header` node (`STRING <id>`) — jump target.
    #[allow(dead_code)]
    pub header_range: Range,
    /// Range of just the `identifier` node — for highlight.
    #[allow(dead_code)]
    pub name_range: Range,
}

/// Per-URI map of TRIGSTR identifiers defined in that WTS file.
///
/// Key: WTS file URI.
/// Value: `HashMap<identifier_text, TrigstrEntry>`.
pub static TRIGSTR_MAP: Lazy<DashMap<Url, HashMap<String, TrigstrEntry>>> = Lazy::new(DashMap::new);
