use log::{error, info};
use once_cell::sync::Lazy;
use petgraph::algo::is_cyclic_directed;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::{Bfs, EdgeRef};
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use url::Url;

// ─── Cache ───────────────────────────────────────────────────────────────────

const CACHE_FILE: &str = "jass-tree-sitter-import-graph.json";

fn cache_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join(CACHE_FILE))
}

/// On-disk representation — just the forward edges.
#[derive(Serialize, Deserialize, Default)]
pub(crate) struct Snapshot {
    pub(crate) edges: HashMap<Url, Vec<Url>>,
}

// ─── ImportGraph ─────────────────────────────────────────────────────────────

/// Global import graph shared by all languages.
pub static IMPORT_GRAPH: Lazy<ImportGraph> = Lazy::new(ImportGraph::load);

/// Directed import graph backed by `petgraph`.
///
/// Each node is a `Url`. An edge `A → B` means "A imports B".
///
/// The graph is persisted to `$CACHE_DIR/jass-tree-sitter-import-graph.json`
/// so it survives server restarts.
pub struct ImportGraph {
    inner: RwLock<GraphInner>,
}

struct GraphInner {
    graph: DiGraph<Url, ()>,
    /// Url → NodeIndex lookup for O(1) access.
    index: HashMap<Url, NodeIndex>,
}

impl GraphInner {
    pub(crate) fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            index: HashMap::new(),
        }
    }

    /// Get or create a node for `uri`.
    fn ensure_node(&mut self, uri: &Url) -> NodeIndex {
        if let Some(&idx) = self.index.get(uri) {
            idx
        } else {
            let idx = self.graph.add_node(uri.clone());
            self.index.insert(uri.clone(), idx);
            idx
        }
    }

    /// All outgoing neighbors of `node` (files that `node` imports).
    fn outgoing(&self, node: NodeIndex) -> Vec<NodeIndex> {
        self.graph
            .neighbors_directed(node, Direction::Outgoing)
            .collect()
    }

    /// Remove all outgoing edges from `node`.
    fn clear_outgoing(&mut self, node: NodeIndex) {
        let edges: Vec<_> = self
            .graph
            .edges_directed(node, Direction::Outgoing)
            .map(|e| e.id())
            .collect();
        for eid in edges {
            self.graph.remove_edge(eid);
        }
    }
}

impl ImportGraph {
    /// Create an empty in-memory graph (for tests).
    #[cfg(test)]
    pub(crate) fn new_empty() -> Self {
        Self {
            inner: RwLock::new(GraphInner::new()),
        }
    }

    /// Load from disk cache, or create empty.
    fn load() -> Self {
        let mut inner = GraphInner::new();

        if let Some(path) = cache_path() {
            if path.exists() {
                match fs::read_to_string(&path) {
                    Ok(data) => match serde_json::from_str::<Snapshot>(&data) {
                        Ok(snap) => {
                            for (from, tos) in &snap.edges {
                                let from_idx = inner.ensure_node(from);
                                for to in tos {
                                    let to_idx = inner.ensure_node(to);
                                    inner.graph.update_edge(from_idx, to_idx, ());
                                }
                            }
                            info!(
                                "import_graph: loaded {} files, {} edges from {}",
                                inner.graph.node_count(),
                                inner.graph.edge_count(),
                                path.display()
                            );
                        }
                        Err(e) => error!("import_graph: parse cache: {}", e),
                    },
                    Err(e) => error!("import_graph: read cache: {}", e),
                }
            }
        }

        Self {
            inner: RwLock::new(inner),
        }
    }

    /// Persist current state to disk.
    fn save(inner: &GraphInner) {
        let Some(path) = cache_path() else { return };

        let mut edges: HashMap<Url, Vec<Url>> = HashMap::new();
        for idx in inner.graph.node_indices() {
            let from = &inner.graph[idx];
            let tos: Vec<Url> = inner
                .graph
                .neighbors_directed(idx, Direction::Outgoing)
                .map(|n| inner.graph[n].clone())
                .collect();
            if !tos.is_empty() {
                edges.insert(from.clone(), tos);
            }
        }

        let snap = Snapshot { edges };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match serde_json::to_string(&snap) {
            Ok(data) => {
                if let Err(e) = fs::write(&path, data) {
                    error!("import_graph: write cache: {}", e);
                }
            }
            Err(e) => error!("import_graph: serialize: {}", e),
        }
    }

    // ─── Mutation ────────────────────────────────────────────────────────

    /// Update the import list for `uri`.
    ///
    /// Replaces all outgoing edges of `uri` with `new_imports`.
    /// Skips disk write if nothing changed.
    pub fn update(&self, uri: &Url, new_imports: HashSet<Url>) {
        let mut inner = self.inner.write().unwrap();
        let node = inner.ensure_node(uri);

        // Collect current outgoing set
        let old: HashSet<Url> = inner
            .outgoing(node)
            .iter()
            .map(|&n| inner.graph[n].clone())
            .collect();

        if old == new_imports {
            return;
        }

        inner.clear_outgoing(node);
        for imp in &new_imports {
            let to = inner.ensure_node(imp);
            inner.graph.update_edge(node, to, ());
        }

        Self::save(&inner);
    }

    /// Remove a file node and all its edges (e.g. file deleted from disk).
    #[allow(dead_code)]
    pub fn remove(&self, uri: &Url) {
        let mut inner = self.inner.write().unwrap();
        if let Some(&idx) = inner.index.get(uri) {
            inner.graph.remove_node(idx);
            inner.index.remove(uri);
            // petgraph may swap indices on remove — rebuild index
            let rebuilt: HashMap<Url, NodeIndex> = inner
                .graph
                .node_indices()
                .map(|idx| (inner.graph[idx].clone(), idx))
                .collect();
            inner.index = rebuilt;
            Self::save(&inner);
        }
    }

    // ─── Queries (read lock) ─────────────────────────────────────────────

    /// All files that **transitively** import `uri` (walk incoming edges).
    ///
    /// If A→B→C, then `dependents(C) = [B, A]`.
    #[allow(dead_code)]
    pub fn dependents(&self, uri: &Url) -> Vec<Url> {
        let inner = self.inner.read().unwrap();
        let Some(&idx) = inner.index.get(uri) else {
            return vec![];
        };
        // BFS on the reversed graph (incoming direction)
        let reversed = petgraph::visit::Reversed(&inner.graph);
        let mut bfs = Bfs::new(&reversed, idx);
        let mut result = Vec::new();
        // Skip the start node itself
        bfs.next(&reversed);
        while let Some(n) = bfs.next(&reversed) {
            result.push(inner.graph[n].clone());
        }
        result
    }

    /// All files that `uri` **transitively** imports (walk outgoing edges).
    ///
    /// If A→B→C, then `dependencies(A) = [B, C]`.
    #[allow(dead_code)]
    pub fn dependencies(&self, uri: &Url) -> Vec<Url> {
        let inner = self.inner.read().unwrap();
        let Some(&idx) = inner.index.get(uri) else {
            return vec![];
        };
        let mut bfs = Bfs::new(&inner.graph, idx);
        let mut result = Vec::new();
        bfs.next(&inner.graph); // skip start
        while let Some(n) = bfs.next(&inner.graph) {
            result.push(inner.graph[n].clone());
        }
        result
    }

    /// Direct (non-transitive) imports of `uri`.
    #[allow(dead_code)]
    pub fn direct_imports(&self, uri: &Url) -> Vec<Url> {
        let inner = self.inner.read().unwrap();
        let Some(&idx) = inner.index.get(uri) else {
            return vec![];
        };
        inner
            .graph
            .neighbors_directed(idx, Direction::Outgoing)
            .map(|n| inner.graph[n].clone())
            .collect()
    }

    /// Direct (non-transitive) dependents of `uri`.
    #[allow(dead_code)]
    pub fn direct_dependents(&self, uri: &Url) -> Vec<Url> {
        let inner = self.inner.read().unwrap();
        let Some(&idx) = inner.index.get(uri) else {
            return vec![];
        };
        inner
            .graph
            .neighbors_directed(idx, Direction::Incoming)
            .map(|n| inner.graph[n].clone())
            .collect()
    }

    /// Detect if the graph contains any cycle.
    #[allow(dead_code)]
    pub fn has_cycle(&self) -> bool {
        let inner = self.inner.read().unwrap();
        is_cyclic_directed(&inner.graph)
    }

    /// Find all cycles that `uri` participates in.
    /// Returns a list of cycles, each cycle is a Vec<Url>.
    #[allow(dead_code)]
    pub fn cycles_for(&self, uri: &Url) -> Vec<Vec<Url>> {
        let inner = self.inner.read().unwrap();
        let Some(&start) = inner.index.get(uri) else {
            return vec![];
        };
        let mut result = Vec::new();
        let mut path = Vec::new();
        let mut visited = HashSet::new();
        Self::dfs_cycles(&inner, start, start, &mut path, &mut visited, &mut result);
        result
    }

    /// DFS to find cycles that return to `target`.
    fn dfs_cycles(
        inner: &GraphInner,
        current: NodeIndex,
        target: NodeIndex,
        path: &mut Vec<NodeIndex>,
        visited: &mut HashSet<NodeIndex>,
        result: &mut Vec<Vec<Url>>,
    ) {
        path.push(current);
        visited.insert(current);

        for next in inner.graph.neighbors_directed(current, Direction::Outgoing) {
            if next == target && path.len() > 1 {
                // Found a cycle back to target
                let cycle: Vec<Url> = path.iter().map(|&n| inner.graph[n].clone()).collect();
                result.push(cycle);
            } else if !visited.contains(&next) {
                Self::dfs_cycles(inner, next, target, path, visited, result);
            }
        }

        path.pop();
        visited.remove(&current);
    }

    /// Topological sort of all nodes. Returns `None` if the graph has a cycle.
    #[allow(dead_code)]
    pub fn toposort(&self) -> Option<Vec<Url>> {
        let inner = self.inner.read().unwrap();
        petgraph::algo::toposort(&inner.graph, None)
            .ok()
            .map(|sorted| sorted.iter().map(|&n| inner.graph[n].clone()).collect())
    }

    /// Number of files in the graph.
    #[allow(dead_code)]
    pub fn node_count(&self) -> usize {
        self.inner.read().unwrap().graph.node_count()
    }

    /// Number of import edges in the graph.
    #[allow(dead_code)]
    pub fn edge_count(&self) -> usize {
        self.inner.read().unwrap().graph.edge_count()
    }
}

// ─── Utility ─────────────────────────────────────────────────────────────────

/// Resolve a relative import path against the directory of `base_uri`.
///
/// Strips surrounding quotes. Returns `None` if path is empty.
pub fn resolve_import(base_uri: &Url, relative_path: &str) -> Option<Url> {
    let path = relative_path.trim().trim_matches('"').trim_matches('\'');
    if path.is_empty() {
        return None;
    }
    let mut base = base_uri.clone();
    base.path_segments_mut().ok()?.pop();
    for segment in path.split('/') {
        if segment == ".." {
            base.path_segments_mut().ok()?.pop();
        } else if segment != "." && !segment.is_empty() {
            base.path_segments_mut().ok()?.push(segment);
        }
    }
    Some(base)
}

