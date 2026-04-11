use crate::http::document_symbol::SymbolKind;
use crate::http::position::Position;
use crate::http::range::Range;
use crate::util::parse_cache::{PARSE_CACHE, peek_or_load};
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_map::LNG_URI_MAP;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

// ─── Types ───────────────────────────────────────────────────────────────────

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#typeHierarchyPrepareParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeHierarchyPrepareParams {
    pub uri: Url,
    pub position: Position,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#typeHierarchyItem
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeHierarchyItem {
    pub name: String,
    pub kind: SymbolKind,
    pub uri: String,
    pub range: Range,
    pub selection_range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#typeHierarchySupertypesParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeHierarchySupertypesParams {
    pub item: TypeHierarchyItem,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#typeHierarchySubtypesParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeHierarchySubtypesParams {
    pub item: TypeHierarchyItem,
}

// ─── Prepare ─────────────────────────────────────────────────────────────────

pub(crate) fn compute_prepare(uri: &Url, position: &Position) -> Option<Vec<TypeHierarchyItem>> {
    let lng = LNG_URI_MAP.get(uri)?;
    let lng_val = lng.value().clone();
    if lng_val != "jass" && lng_val != "angelscript" {
        return None;
    }

    let snapshot = PARSE_CACHE.get(uri)?;
    let snap = snapshot.value();
    let ref_map = &snap.ref_map;

    let rope_entry = ROPE_MAP.get(uri)?;
    let byte_offset = position.to_byte_offset(rope_entry.value())?;

    // Find the symbol at cursor
    let span = ref_map.spans.iter().find(|s| {
        byte_offset >= s.start_byte && byte_offset < s.end_byte
    })?;
    let name = ref_map.groups.get(&span.decl_key)?.name.clone();

    // Check if it's a type
    let is_type = snap.file_symbols.find_type(&name).is_some();

    if !is_type {
        // Check external
        if let Some(ext) = ref_map.external_decls.get(&span.decl_key) {
            let mut found = false;
            for origin in &ext.origins {
                if let Some(ext_snap) = peek_or_load(&origin.uri) {
                    if ext_snap.file_symbols.find_type(&ext.name).is_some() {
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                return None;
            }
        } else {
            return None;
        }
    }

    let decl_range = find_type_decl_range(uri, &name)?;

    let detail = snap
        .file_symbols
        .find_type(&name)
        .and_then(|t| t.base.as_ref().map(|b| format!("extends {}", b)));

    Some(vec![TypeHierarchyItem {
        name: name.clone(),
        kind: SymbolKind::Class,
        uri: uri.to_string(),
        range: decl_range.clone(),
        selection_range: span.range.clone(),
        detail,
        data: None,
    }])
}

// ─── Supertypes ──────────────────────────────────────────────────────────────

pub(crate) fn compute_supertypes(item: &TypeHierarchyItem) -> Vec<TypeHierarchyItem> {
    let type_name = &item.name;
    let item_uri = match Url::parse(&item.uri) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let mut component = IMPORT_GRAPH.visible_component(&item_uri);
    component.insert(item_uri.clone());

    // Find the base type
    let base_name = find_base_type(&component, type_name);
    let base_name = match base_name {
        Some(b) => b,
        None => return vec![],
    };

    // Find where the base type is declared
    for peer_uri in &component {
        if let Some(snap) = peek_or_load(peer_uri) {
            if let Some(t) = snap.file_symbols.find_type(&base_name) {
                if let Some(range) = find_type_decl_range(peer_uri, &base_name) {
                    let detail = t.base.as_ref().map(|b| format!("extends {}", b));
                    return vec![TypeHierarchyItem {
                        name: base_name,
                        kind: SymbolKind::Class,
                        uri: peer_uri.to_string(),
                        range: range.clone(),
                        selection_range: range,
                        detail,
                        data: None,
                    }];
                }
            }
        }
    }

    vec![]
}

// ─── Subtypes ────────────────────────────────────────────────────────────────

pub(crate) fn compute_subtypes(item: &TypeHierarchyItem) -> Vec<TypeHierarchyItem> {
    let type_name = &item.name;
    let item_uri = match Url::parse(&item.uri) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let mut component = IMPORT_GRAPH.visible_component(&item_uri);
    component.insert(item_uri.clone());

    let mut subtypes = Vec::new();

    for peer_uri in &component {
        if let Some(snap) = peek_or_load(peer_uri) {
            for t in &snap.file_symbols.types {
                if t.base.as_deref() == Some(type_name) {
                    if let Some(range) = find_type_decl_range(peer_uri, &t.name) {
                        let detail = Some(format!("extends {}", type_name));
                        subtypes.push(TypeHierarchyItem {
                            name: t.name.clone(),
                            kind: SymbolKind::Class,
                            uri: peer_uri.to_string(),
                            range: range.clone(),
                            selection_range: range,
                            detail,
                            data: None,
                        });
                    }
                }
            }
        }
    }

    subtypes
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Find the base type of a given type across all files in the component.
fn find_base_type(
    component: &std::collections::HashSet<Url>,
    type_name: &str,
) -> Option<String> {
    for peer_uri in component {
        if let Some(snap) = peek_or_load(peer_uri) {
            if let Some(t) = snap.file_symbols.find_type(type_name) {
                return t.base.clone();
            }
        }
    }
    None
}

/// Find the declaration range for a type in a file.
fn find_type_decl_range(uri: &Url, name: &str) -> Option<Range> {
    let snap = peek_or_load(uri)?;
    let ref_map = &snap.ref_map;

    for group in ref_map.groups.values() {
        if group.name == name {
            if let Some(occ) = group.occurrences.iter().find(|o| o.is_decl) {
                return Some(occ.range.clone());
            }
        }
    }

    None
}

