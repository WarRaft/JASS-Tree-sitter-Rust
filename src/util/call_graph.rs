//! Build a cross-file **function call graph** for all files in the
//! connected component of a given URI.
//!
//! The graph is a `petgraph::DiGraph` where each node is a function
//! (or native) and each edge represents a direct call.  It answers the
//! question: **can we arrange functions so that every callee is declared
//! before its caller?**  (JASS forward-declaration requirement.)
//!
//! * **Topological order** — a valid build order if one exists.
//! * **Cycle detection** — via Tarjan SCC; non-trivial SCCs prevent ordering.
//! * **Unused detection** — functions with zero incoming edges (nobody calls them).
//! * **Recursion detection** — self-edges (allowed, but flagged).

use crate::lng::jass::symbol::FILE_SYMBOLS;
use crate::util::import_graph::IMPORT_GRAPH;
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use url::Url;

// ─── Diagnostic helpers ──────────────────────────────────────────────────────

/// Per-function diagnostic info for a single file.
#[derive(Debug, Default)]
pub struct FuncDiagnostics {
    /// Function names declared in this file that nobody calls.
    pub unused: HashSet<String>,
    /// Function names declared in this file that participate in a
    /// non-trivial call cycle (SCC len > 1) preventing topological ordering.
    pub in_cycle: HashSet<String>,
}

/// Lightweight analysis: return unused / cyclic function names declared in `uri`.
///
/// Reads `FILE_SYMBOLS` for every file in the connected component that
/// contains `uri` (must already be populated).
pub fn diagnose_functions(uri: &Url) -> FuncDiagnostics {
    let mut result = FuncDiagnostics::default();

    let mut component: HashSet<Url> = IMPORT_GRAPH.connected_component(uri);
    component.insert(uri.clone());

    // Frozen URIs.
    let mut frozen_uris: HashSet<Url> = HashSet::new();
    for peer in &component {
        if let Some(fs) = FILE_SYMBOLS.get(peer) {
            for fu in &fs.frozen_imports {
                frozen_uris.insert(fu.clone());
            }
        }
    }

    // Collect all functions/natives.
    #[allow(dead_code)]
    struct Info {
        uri: Url,
        is_native: bool,
        is_frozen: bool,
        callees: HashSet<String>,
    }

    let mut func_map: HashMap<String, Info> = HashMap::new();

    for peer_uri in &component {
        let fs = match FILE_SYMBOLS.get(peer_uri) {
            Some(fs) => fs,
            None => continue,
        };
        let is_frozen = frozen_uris.contains(peer_uri);

        for f in &fs.functions {
            func_map.insert(f.name.clone(), Info {
                uri: peer_uri.clone(),
                is_native: false,
                is_frozen,
                callees: f.callees.clone(),
            });
        }
        for n in &fs.natives {
            func_map.insert(n.name.clone(), Info {
                uri: peer_uri.clone(),
                is_native: true,
                is_frozen,
                callees: HashSet::new(),
            });
        }
    }

    // Build graph.
    let mut graph: DiGraph<String, ()> = DiGraph::new();
    let mut name_to_idx: HashMap<String, NodeIndex> = HashMap::new();

    for name in func_map.keys() {
        let idx = graph.add_node(name.clone());
        name_to_idx.insert(name.clone(), idx);
    }
    for (name, info) in &func_map {
        if let Some(&caller) = name_to_idx.get(name) {
            for callee_name in &info.callees {
                if let Some(&callee) = name_to_idx.get(callee_name) {
                    graph.add_edge(caller, callee, ());
                }
            }
        }
    }

    // In-degree (excluding self-loops) → unused.
    let mut in_degree: HashMap<NodeIndex, usize> = HashMap::new();
    for ni in graph.node_indices() {
        in_degree.insert(ni, 0);
    }
    for edge in graph.edge_indices() {
        if let Some((s, t)) = graph.edge_endpoints(edge) {
            if s != t {
                *in_degree.entry(t).or_insert(0) += 1;
            }
        }
    }

    // Tarjan SCC → cycles.
    let sccs = tarjan_scc(&graph);
    let mut cycle_nodes: HashSet<NodeIndex> = HashSet::new();
    for scc in &sccs {
        if scc.len() > 1 {
            for &ni in scc {
                cycle_nodes.insert(ni);
            }
        }
    }

    // Filter to functions declared in `uri`.
    for (name, info) in &func_map {
        if info.uri != *uri || info.is_native {
            continue;
        }
        if let Some(&ni) = name_to_idx.get(name) {
            if *in_degree.get(&ni).unwrap_or(&0) == 0 {
                result.unused.insert(name.clone());
            }
            if cycle_nodes.contains(&ni) {
                result.in_cycle.insert(name.clone());
            }
        }
    }

    result
}

// ─── Result types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct CallGraphNode {
    /// Display name of the function / native.
    pub name: String,
    /// URI of the file that declares this function (for navigation).
    pub uri: String,
    /// `true` when the function comes from a `//import!` file.
    pub is_frozen: bool,
    /// `true` when the function calls itself (self-edge).
    pub is_recursive: bool,
    /// `true` when the function is part of a non-trivial cycle (SCC len > 1).
    pub in_cycle: bool,
    /// `true` when nobody calls this function (zero incoming non-self edges).
    pub is_unused: bool,
    /// `true` when this is a `native` (no body, cannot be reordered).
    pub is_native: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallGraphResult {
    /// Nodes (index = stable id used in edges / topo_order / cycles).
    pub nodes: Vec<CallGraphNode>,
    /// `[caller_idx, callee_idx]` — caller calls callee.
    pub edges: Vec<[usize; 2]>,
    /// Valid build order (callees first).  Non-empty even when cycles exist
    /// (best-effort), but `is_orderable` tells whether it is truly valid.
    pub topo_order: Vec<usize>,
    /// `true` when a valid ordering exists (no non-trivial cycles).
    pub is_orderable: bool,
    /// Groups of node indices that form non-trivial cycles.
    pub cycles: Vec<Vec<usize>>,
}

// ─── Builder ─────────────────────────────────────────────────────────────────

/// Build the call graph for the connected component that includes `uri`.
pub fn build_call_graph(uri: &Url) -> CallGraphResult {
    // All files connected via imports (+ self).
    let mut component: HashSet<Url> = IMPORT_GRAPH.connected_component(uri);
    component.insert(uri.clone());

    // Frozen URIs — any file imported via `//import!` by anyone in component.
    let mut frozen_uris: HashSet<Url> = HashSet::new();
    for peer in &component {
        if let Some(fs) = FILE_SYMBOLS.get(peer) {
            for fu in &fs.frozen_imports {
                frozen_uris.insert(fu.clone());
            }
        }
    }

    // Collect all declared functions / natives.
    struct FuncInfo {
        uri: Url,
        is_native: bool,
        is_frozen: bool,
        callees: HashSet<String>,
    }

    let mut func_map: HashMap<String, FuncInfo> = HashMap::new();

    for peer_uri in &component {
        let fs = match FILE_SYMBOLS.get(peer_uri) {
            Some(fs) => fs,
            None => continue,
        };
        let is_frozen = frozen_uris.contains(peer_uri);

        for f in &fs.functions {
            func_map.insert(
                f.name.clone(),
                FuncInfo {
                    uri: peer_uri.clone(),
                    is_native: false,
                    is_frozen,
                    callees: f.callees.clone(),
                },
            );
        }
        for n in &fs.natives {
            func_map.insert(
                n.name.clone(),
                FuncInfo {
                    uri: peer_uri.clone(),
                    is_native: true,
                    is_frozen,
                    callees: HashSet::new(),
                },
            );
        }
    }

    // Determine which frozen functions are actually referenced by non-frozen code.
    let mut referenced_frozen: HashSet<String> = HashSet::new();
    for info in func_map.values() {
        if !info.is_frozen {
            for callee_name in &info.callees {
                if let Some(ci) = func_map.get(callee_name) {
                    if ci.is_frozen {
                        referenced_frozen.insert(callee_name.clone());
                    }
                }
            }
        }
    }

    // ── Build petgraph ──────────────────────────────────────────────────

    let mut graph: DiGraph<String, ()> = DiGraph::new();
    let mut name_to_idx: HashMap<String, NodeIndex> = HashMap::new();
    let mut idx_to_pos: HashMap<NodeIndex, usize> = HashMap::new();

    // Stable alphabetical order for deterministic output.
    let mut ordered_names: Vec<String> = func_map.keys().cloned().collect();
    ordered_names.sort();

    // Add nodes — skip frozen functions that nobody references.
    for name in &ordered_names {
        let info = &func_map[name];
        if info.is_frozen && !referenced_frozen.contains(name) {
            continue;
        }
        let idx = graph.add_node(name.clone());
        idx_to_pos.insert(idx, name_to_idx.len());
        name_to_idx.insert(name.clone(), idx);
    }

    // Add edges: caller → callee.
    for (name, info) in &func_map {
        let caller_idx = match name_to_idx.get(name) {
            Some(&i) => i,
            None => continue,
        };
        for callee_name in &info.callees {
            if let Some(&callee_idx) = name_to_idx.get(callee_name) {
                graph.add_edge(caller_idx, callee_idx, ());
            }
        }
    }

    // ── Analysis ────────────────────────────────────────────────────────

    // Self-loops → recursion.
    let mut self_loops: HashSet<NodeIndex> = HashSet::new();
    for edge in graph.edge_indices() {
        if let Some((s, t)) = graph.edge_endpoints(edge) {
            if s == t {
                self_loops.insert(s);
            }
        }
    }

    // Tarjan SCC → cycle detection.
    let sccs = tarjan_scc(&graph);
    let mut cycle_nodes: HashSet<NodeIndex> = HashSet::new();
    let mut cycle_groups: Vec<Vec<usize>> = Vec::new();

    for scc in &sccs {
        if scc.len() > 1 {
            for &ni in scc {
                cycle_nodes.insert(ni);
            }
            cycle_groups.push(scc.iter().map(|ni| idx_to_pos[ni]).collect());
        }
    }

    let is_orderable = cycle_groups.is_empty();

    // Best-effort topological order from SCC (Tarjan gives reverse topo).
    // Callees-first: the function that is called appears earlier in the list.
    let topo_order: Vec<usize> = sccs
        .iter()
        .rev()
        .flat_map(|scc| scc.iter().map(|ni| idx_to_pos[ni]))
        .collect();

    // In-degree (excluding self-loops) → unused detection.
    let mut in_degree: HashMap<NodeIndex, usize> = HashMap::new();
    for ni in graph.node_indices() {
        in_degree.insert(ni, 0);
    }
    for edge in graph.edge_indices() {
        if let Some((s, t)) = graph.edge_endpoints(edge) {
            if s != t {
                *in_degree.entry(t).or_insert(0) += 1;
            }
        }
    }

    // ── Assemble result ─────────────────────────────────────────────────

    let mut index_order: Vec<(NodeIndex, usize)> =
        idx_to_pos.iter().map(|(&ni, &pos)| (ni, pos)).collect();
    index_order.sort_by_key(|&(_, pos)| pos);

    let mut nodes: Vec<CallGraphNode> = Vec::new();
    for &(ni, _) in &index_order {
        let name = &graph[ni];
        let info = &func_map[name];
        nodes.push(CallGraphNode {
            name: name.clone(),
            uri: info.uri.to_string(),
            is_frozen: info.is_frozen,
            is_recursive: self_loops.contains(&ni),
            in_cycle: cycle_nodes.contains(&ni),
            is_unused: *in_degree.get(&ni).unwrap_or(&0) == 0 && !info.is_native,
            is_native: info.is_native,
        });
    }

    let mut edges_out: Vec<[usize; 2]> = Vec::new();
    for edge in graph.edge_indices() {
        if let Some((s, t)) = graph.edge_endpoints(edge) {
            edges_out.push([idx_to_pos[&s], idx_to_pos[&t]]);
        }
    }

    CallGraphResult {
        nodes,
        edges: edges_out,
        topo_order,
        is_orderable,
        cycles: cycle_groups,
    }
}
