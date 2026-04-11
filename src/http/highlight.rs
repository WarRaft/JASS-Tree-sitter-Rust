use crate::http::position::Position;
use crate::http::range::Range;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use url::Url;


#[derive(Debug, Serialize_repr, Deserialize_repr, Clone, Copy)]
#[repr(u8)]
pub enum DocumentHighlightKind {
    /// A textual occurrence.
    Text = 1,
    /// Read-access of a symbol.
    Read = 2,
    /// Write-access of a symbol.
    Write = 3,
}

#[derive(Debug, Serialize, Clone)]
pub struct DocumentHighlight {
    pub range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<DocumentHighlightKind>,
}

#[derive(Debug, Deserialize)]
pub struct DefinitionParams {
    pub uri: Url,
    pub position: Position,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceParams {
    pub uri: Url,
    pub position: Position,
    #[serde(default)]
    pub context: ReferenceContext,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceContext {
    pub include_declaration: bool,
}

pub(crate) fn compute_highlight(uri: &Url, position: &Position) -> Vec<DocumentHighlight> {
    let snapshot = match crate::util::parse_cache::PARSE_CACHE.get(uri) {
        Some(s) => s,
        None => return vec![],
    };
    let ref_map = &snapshot.ref_map;

    let rope_entry = match crate::util::roper::uri_map::ROPE_MAP.get(uri) {
        Some(r) => r,
        None => return vec![],
    };
    let byte_offset = match position.to_byte_offset(rope_entry.value()) {
        Some(o) => o,
        None => return vec![],
    };

    ref_map
        .occurrences_at(byte_offset)
        .iter()
        .map(|occ| DocumentHighlight {
            range: occ.range.clone(),
            kind: Some(occ.kind),
        })
        .collect()
}

