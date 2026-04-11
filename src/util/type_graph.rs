//! Build a **type inheritance graph** for all files in the connected
//! component of a given URI.
//!
//! JASS type declarations look like `type Foo extends Bar`.  The graph
//! is rooted at `handle` — the hardcoded base type for all game objects.
//! Types with no explicit base (or with base `handle`) are children of
//! the root `handle` node.

use crate::util::parse_cache::PARSE_CACHE;
use crate::util::import_graph::IMPORT_GRAPH;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use url::Url;

/// The hardcoded root type in JASS.
const ROOT_TYPE: &str = "handle";

/// Node in the type graph result.
#[derive(Debug, Clone, Serialize)]
pub struct TypeGraphNode {
    pub name: String,
    pub uri: String,
    /// `true` for the synthetic root node (`handle`) when it has no
    /// explicit declaration in any file.
    pub is_root: bool,
    /// `true` when the node comes from a frozen (`//import!`) file.
    pub is_frozen: bool,
}

/// Result of `build_type_graph`.
#[derive(Debug, Clone, Serialize)]
pub struct TypeGraphResult {
    pub nodes: Vec<TypeGraphNode>,
    /// Directed edges `[parent_idx, child_idx]` — from base type to derived.
    pub edges: Vec<[usize; 2]>,
}

/// Build a type inheritance graph for the connected component of `uri`.
pub fn build_type_graph(uri: &Url) -> TypeGraphResult {
    let mut component: HashSet<Url> = IMPORT_GRAPH.visible_component(uri);
    component.insert(uri.clone());

    // Frozen URIs.
    let mut frozen_uris: HashSet<Url> = HashSet::new();
    for peer in &component {
        if let Some(fs) = PARSE_CACHE.get(peer) {
            for fu in &fs.file_symbols.frozen_imports {
                frozen_uris.insert(fu.clone());
            }
        }
    }

    // Collect all types: name → (base, uri, is_frozen).
    let mut type_map: HashMap<String, (Option<String>, String, bool)> = HashMap::new();

    for peer_uri in &component {
        let fs = match PARSE_CACHE.get(peer_uri) {
            Some(fs) => fs,
            None => continue,
        };
        let is_frozen = frozen_uris.contains(peer_uri);

        for t in &fs.file_symbols.types {
            type_map.entry(t.name.clone()).or_insert_with(|| {
                (t.base.clone(), peer_uri.to_string(), is_frozen)
            });
        }
    }

    // Ensure root node exists.
    let has_handle = type_map.contains_key(ROOT_TYPE);

    // Build nodes list.  Index 0 = root (`handle`).
    let mut nodes: Vec<TypeGraphNode> = Vec::new();
    let mut name_to_idx: HashMap<String, usize> = HashMap::new();

    // Root node.
    if has_handle {
        let (_, ref uri_str, is_frozen) = type_map[ROOT_TYPE];
        nodes.push(TypeGraphNode {
            name: ROOT_TYPE.to_string(),
            uri: uri_str.clone(),
            is_root: true,
            is_frozen,
        });
    } else {
        nodes.push(TypeGraphNode {
            name: ROOT_TYPE.to_string(),
            uri: String::new(),
            is_root: true,
            is_frozen: false,
        });
    }
    name_to_idx.insert(ROOT_TYPE.to_string(), 0);

    // Add remaining types in sorted order for determinism.
    let mut sorted_names: Vec<&String> = type_map.keys().collect();
    sorted_names.sort();

    for name in &sorted_names {
        if name.as_str() == ROOT_TYPE {
            continue;
        }
        let idx = nodes.len();
        let (_, ref uri_str, is_frozen) = type_map[name.as_str()];
        nodes.push(TypeGraphNode {
            name: name.to_string(),
            uri: uri_str.clone(),
            is_root: false,
            is_frozen,
        });
        name_to_idx.insert(name.to_string(), idx);
    }

    // Build edges: base → derived.
    let mut edges: Vec<[usize; 2]> = Vec::new();

    for name in &sorted_names {
        if name.as_str() == ROOT_TYPE {
            continue;
        }
        let (ref base_opt, _, _) = type_map[name.as_str()];
        let child_idx = name_to_idx[name.as_str()];

        let parent_idx = match base_opt {
            Some(base) => {
                if let Some(&idx) = name_to_idx.get(base.as_str()) {
                    idx
                } else {
                    // Unknown base — create a synthetic node for it.
                    let idx = nodes.len();
                    nodes.push(TypeGraphNode {
                        name: base.clone(),
                        uri: String::new(),
                        is_root: false,
                        is_frozen: false,
                    });
                    name_to_idx.insert(base.clone(), idx);
                    idx
                }
            }
            // No base → child of handle.
            None => 0,
        };

        edges.push([parent_idx, child_idx]);
    }

    TypeGraphResult { nodes, edges }
}

