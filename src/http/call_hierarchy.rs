use crate::http::document_symbol::SymbolKind;
use crate::http::position::Position;
use crate::http::range::Range;
use crate::util::parse_cache::peek_or_load;
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_map::LNG_URI_MAP;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CallHierarchyPrepareParams {
    pub uri: Url,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallHierarchyItem {
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

#[derive(Debug, Deserialize)]
pub struct CallHierarchyIncomingCallsParams {
    pub item: CallHierarchyItem,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallHierarchyIncomingCall {
    pub from: CallHierarchyItem,
    pub from_ranges: Vec<Range>,
}

#[derive(Debug, Deserialize)]
pub struct CallHierarchyOutgoingCallsParams {
    pub item: CallHierarchyItem,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallHierarchyOutgoingCall {
    pub to: CallHierarchyItem,
    pub from_ranges: Vec<Range>,
}

// ─── Prepare ─────────────────────────────────────────────────────────────────

pub(crate) fn compute_prepare(uri: &Url, position: &Position) -> Option<Vec<CallHierarchyItem>> {
    let lng = LNG_URI_MAP.get(uri)?;
    let lng_val = lng.value().clone();
    if lng_val != "jass" && lng_val != "angelscript" {
        return None;
    }

    let snapshot = peek_or_load(uri)?;
    let snap = &*snapshot;
    let ref_map = &snap.ref_map;

    let rope_entry = ROPE_MAP.get(uri)?;
    let byte_offset = position.to_byte_offset(rope_entry.value())?;

    // Find the symbol at cursor
    let span = ref_map.spans.iter().find(|s| {
        byte_offset >= s.start_byte && byte_offset < s.end_byte
    })?;
    let name = ref_map.groups.get(&span.decl_key)?.name.clone();

    // Check if it's a function/native (in func_decl_keys or external func)
    let is_func = snap.func_decl_keys.contains(&span.decl_key)
        || {
            let fs = &snap.file_symbols;
            fs.find_function(&name).is_some() || fs.find_native(&name).is_some()
        };

    if !is_func {
        if let Some(ext) = ref_map.external_decls.get(&span.decl_key) {
            let mut found = false;
            for origin in &ext.origins {
                if let Some(ext_snap) = peek_or_load(&origin.uri) {
                    let fs = &ext_snap.file_symbols;
                    if fs.find_function(&ext.name).is_some()
                        || fs.find_native(&ext.name).is_some()
                    {
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

    let decl_range = find_func_decl_range(uri, &name)?;

    Some(vec![CallHierarchyItem {
        name: name.clone(),
        kind: SymbolKind::Function,
        uri: uri.to_string(),
        range: decl_range.clone(),
        selection_range: span.range.clone(),
        detail: None,
        data: None,
    }])
}

// ─── Incoming Calls ──────────────────────────────────────────────────────────

pub(crate) fn compute_incoming(item: &CallHierarchyItem) -> Vec<CallHierarchyIncomingCall> {
    let target_name = &item.name;
    let item_uri = match Url::parse(&item.uri) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let mut component = IMPORT_GRAPH.visible_component(&item_uri);
    component.insert(item_uri.clone());

    let mut calls = Vec::new();

    for peer_uri in &component {
        if let Some(snap_entry) = peek_or_load(peer_uri) {
            let fs = &snap_entry.file_symbols;

            for func in &fs.functions {
                if func.callees.contains(target_name) {
                    if let Some(decl_range) = find_func_decl_range(peer_uri, &func.name) {
                        let call_ranges = find_call_ranges(peer_uri, target_name);

                        calls.push(CallHierarchyIncomingCall {
                            from: CallHierarchyItem {
                                name: func.name.clone(),
                                kind: SymbolKind::Function,
                                uri: peer_uri.to_string(),
                                range: decl_range.clone(),
                                selection_range: decl_range,
                                detail: None,
                                data: None,
                            },
                            from_ranges: call_ranges,
                        });
                    }
                }
            }
        }
    }

    calls
}

// ─── Outgoing Calls ──────────────────────────────────────────────────────────

pub(crate) fn compute_outgoing(item: &CallHierarchyItem) -> Vec<CallHierarchyOutgoingCall> {
    let func_name = &item.name;
    let item_uri = match Url::parse(&item.uri) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let snap_entry = match peek_or_load(&item_uri) {
        Some(s) => s,
        None => return vec![],
    };

    let fs = &snap_entry.file_symbols;

    let callees = match fs.find_function(func_name) {
        Some(f) => &f.callees,
        None => return vec![],
    };

    let mut component = IMPORT_GRAPH.visible_component(&item_uri);
    component.insert(item_uri.clone());

    let mut calls = Vec::new();

    for callee_name in callees {
        let mut callee_uri = None;
        let mut callee_range = None;

        for peer_uri in &component {
            if let Some(peer_snap) = peek_or_load(peer_uri) {
                let pfs = &peer_snap.file_symbols;
                if pfs.find_function(callee_name).is_some()
                    || pfs.find_native(callee_name).is_some()
                {
                    if let Some(range) = find_func_decl_range(peer_uri, callee_name) {
                        callee_uri = Some(peer_uri.clone());
                        callee_range = Some(range);
                        break;
                    }
                }
            }
        }

        if let (Some(cu), Some(cr)) = (callee_uri, callee_range) {
            let call_ranges = find_call_ranges(&item_uri, callee_name);

            calls.push(CallHierarchyOutgoingCall {
                to: CallHierarchyItem {
                    name: callee_name.clone(),
                    kind: SymbolKind::Function,
                    uri: cu.to_string(),
                    range: cr.clone(),
                    selection_range: cr,
                    detail: None,
                    data: None,
                },
                from_ranges: call_ranges,
            });
        }
    }

    calls
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn find_func_decl_range(uri: &Url, name: &str) -> Option<Range> {
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

fn find_call_ranges(uri: &Url, name: &str) -> Vec<Range> {
    let snap = match peek_or_load(uri) {
        Some(s) => s,
        None => return vec![],
    };
    let ref_map = &snap.ref_map;

    for group in ref_map.groups.values() {
        if group.name == name {
            return group
                .occurrences
                .iter()
                .filter(|o| !o.is_decl)
                .map(|o| o.range.clone())
                .collect();
        }
    }

    vec![]
}

