use crate::lsp::call_hierarchy::lsp::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall,
};
use crate::lsp::cancel::CancelId;
use crate::lsp::document_symbol::lsp::SymbolKind;
use crate::lsp::position::Position;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use crate::util::file_store::FILE_STORE;
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_map::LNG_URI_MAP;
use serde_json::Value;
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;
use url::Url;

// ─── Prepare ─────────────────────────────────────────────────────────────────

pub async fn send_prepare(
    writer: &Arc<Mutex<Stdout>>,
    id: Option<CancelId>,
    uri: &Url,
    position: &Position,
) {
    let result = compute_prepare(uri, position);

    lsp_send(
        writer,
        &ResponseMessage::<Value> {
            jsonrpc: "2.0".into(),
            id,
            result: Some(match result {
                Some(items) => serde_json::to_value(&items).unwrap_or(Value::Null),
                None => Value::Null,
            }),
            error: None,
        },
    )
    .await;
}

fn compute_prepare(uri: &Url, position: &Position) -> Option<Vec<CallHierarchyItem>> {
    let lng = LNG_URI_MAP.get(uri)?;
    let lng_val = lng.value().clone();
    if lng_val != "jass" && lng_val != "angelscript" {
        return None;
    }

    let snapshot = FILE_STORE.get(uri)?;
    let snap = snapshot.value();
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
            // Check via file_symbols
            let fs = &snap.file_symbols;
            fs.find_function(&name).is_some() || fs.find_native(&name).is_some()
        };

    if !is_func {
        // For external symbols, check if it's a function
        if let Some(ext) = ref_map.external_decls.get(&span.decl_key) {
            let mut found = false;
            for origin in &ext.origins {
                if let Some(ext_snap) = FILE_STORE.get(&origin.uri) {
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

    // Find the declaration range
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

pub async fn send_incoming(
    writer: &Arc<Mutex<Stdout>>,
    id: Option<CancelId>,
    item: &CallHierarchyItem,
) {
    let result = compute_incoming(item);

    lsp_send(
        writer,
        &ResponseMessage::<Value> {
            jsonrpc: "2.0".into(),
            id,
            result: Some(serde_json::to_value(&result).unwrap_or(Value::Null)),
            error: None,
        },
    )
    .await;
}

fn compute_incoming(item: &CallHierarchyItem) -> Vec<CallHierarchyIncomingCall> {
    let target_name = &item.name;
    let item_uri = match Url::parse(&item.uri) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let mut component = IMPORT_GRAPH.visible_component(&item_uri);
    component.insert(item_uri.clone());

    let mut calls = Vec::new();

    for peer_uri in &component {
        if let Some(snap_entry) = FILE_STORE.get(peer_uri) {
            let fs = &snap_entry.file_symbols;

            for func in &fs.functions {
                if func.callees.contains(target_name) {
                    // This function calls target_name
                    if let Some(decl_range) = find_func_decl_range(peer_uri, &func.name) {
                        // Find the call site ranges in the caller's ref_map
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

pub async fn send_outgoing(
    writer: &Arc<Mutex<Stdout>>,
    id: Option<CancelId>,
    item: &CallHierarchyItem,
) {
    let result = compute_outgoing(item);

    lsp_send(
        writer,
        &ResponseMessage::<Value> {
            jsonrpc: "2.0".into(),
            id,
            result: Some(serde_json::to_value(&result).unwrap_or(Value::Null)),
            error: None,
        },
    )
    .await;
}

fn compute_outgoing(item: &CallHierarchyItem) -> Vec<CallHierarchyOutgoingCall> {
    let func_name = &item.name;
    let item_uri = match Url::parse(&item.uri) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let snap_entry = match FILE_STORE.get(&item_uri) {
        Some(s) => s,
        None => return vec![],
    };

    let fs = &snap_entry.file_symbols;

    // Get the callees of this function
    let callees = match fs.find_function(func_name) {
        Some(f) => &f.callees,
        None => return vec![],
    };

    let mut component = IMPORT_GRAPH.visible_component(&item_uri);
    component.insert(item_uri.clone());

    let mut calls = Vec::new();

    for callee_name in callees {
        // Find where the callee is declared
        let mut callee_uri = None;
        let mut callee_range = None;

        for peer_uri in &component {
            if let Some(peer_snap) = FILE_STORE.get(peer_uri) {
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

/// Find the declaration range for a function/native in a file.
fn find_func_decl_range(uri: &Url, name: &str) -> Option<crate::lsp::range::Range> {
    let snap = FILE_STORE.get(uri)?;
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

/// Find all non-declaration ranges for a symbol name in a file
/// (i.e., call sites / references).
fn find_call_ranges(uri: &Url, name: &str) -> Vec<crate::lsp::range::Range> {
    let snap = match FILE_STORE.get(uri) {
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

