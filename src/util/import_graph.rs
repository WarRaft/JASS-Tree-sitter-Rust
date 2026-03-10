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

    /// Rename a node from `old_uri` to `new_uri`, preserving all edges.
    ///
    /// If `old_uri` doesn't exist this is a no-op.  If `new_uri` already
    /// exists its edges are merged with the renamed node's.
    pub fn rename_node(&self, old_uri: &Url, new_uri: &Url) {
        let mut inner = self.inner.write().unwrap();
        let Some(&old_idx) = inner.index.get(old_uri) else {
            return;
        };

        // Update the node weight.
        inner.graph[old_idx] = new_uri.clone();
        inner.index.remove(old_uri);
        inner.index.insert(new_uri.clone(), old_idx);

        Self::save(&inner);
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

    /// Return the **connected subgraph** reachable from `uri` walking both
    /// outgoing (dependencies) and incoming (dependents) edges.
    ///
    /// The result is a pair `(nodes, edges)` where each node is a URL string
    /// and each edge is `(source_index, target_index)` into the nodes vec.
    /// `nodes[0]` is always `uri` itself (when it exists in the graph).
    pub fn subgraph_for(&self, uri: &Url) -> (Vec<String>, Vec<(usize, usize)>) {
        let inner = self.inner.read().unwrap();
        let Some(&start) = inner.index.get(uri) else {
            return (vec![uri.to_string()], vec![]);
        };

        // BFS in both directions to collect all reachable nodes.
        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(start);
        visited.insert(start);

        while let Some(cur) = queue.pop_front() {
            for next in inner.graph.neighbors_directed(cur, Direction::Outgoing) {
                if visited.insert(next) {
                    queue.push_back(next);
                }
            }
            for next in inner.graph.neighbors_directed(cur, Direction::Incoming) {
                if visited.insert(next) {
                    queue.push_back(next);
                }
            }
        }

        // Build nodes list; start node is always index 0.
        let mut node_list: Vec<NodeIndex> = Vec::with_capacity(visited.len());
        let mut idx_map: HashMap<NodeIndex, usize> = HashMap::new();

        node_list.push(start);
        idx_map.insert(start, 0);

        for &ni in &visited {
            if ni != start {
                idx_map.insert(ni, node_list.len());
                node_list.push(ni);
            }
        }

        let nodes: Vec<String> = node_list
            .iter()
            .map(|&ni| inner.graph[ni].to_string())
            .collect();

        // Collect all edges within the subgraph.
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for &ni in &node_list {
            for e in inner.graph.edges_directed(ni, Direction::Outgoing) {
                let target = e.target();
                if let (Some(&si), Some(&ti)) = (idx_map.get(&ni), idx_map.get(&target)) {
                    edges.push((si, ti));
                }
            }
        }

        (nodes, edges)
    }
}

// ─── Utility ─────────────────────────────────────────────────────────────────

/// Result of [`resolve_import`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImport {
    /// Normalised `file://` URL of the target.
    pub url: Url,
    /// `true` when the file actually exists on disk.
    pub exists: bool,
}

/// Resolve an import path against the directory of `base_uri`.
///
/// # Normalisation rules
///
/// * **Slashes** — `/` and `\` are treated identically.
/// * **Quotes** — surrounding `"` or `'` are stripped.
/// * **Consecutive slashes** are collapsed.
/// * `.` segments are removed, `..` pops the parent (the OS decides
///   what is valid — `..` is **not** artificially clamped at the root).
/// * **Absolute paths** are used as-is:
///   - Windows drive letters: `C:/…`, `C:\…`, `C://…`, `C:\\…`
///   - Unix absolute paths: `/path/to/file`
/// * **Relative paths** are joined to the directory of `base_uri`.
///
/// The resulting URL is always built via pure string normalisation
/// (cross-platform, no `Url::from_file_path`).  A **separate** filesystem
/// check sets [`ResolvedImport::exists`].
///
/// Returns `None` only when the path is empty after trimming.
pub fn resolve_import(base_uri: &Url, raw_path: &str) -> Option<ResolvedImport> {
    let path = raw_path.trim().trim_matches('"').trim_matches('\'').trim();
    if path.is_empty() {
        return None;
    }

    // Normalise all backslashes to forward-slashes.
    let norm = path.replace('\\', "/");

    // Detect absolute path ---------------------------------------------------
    // Windows drive letter: "C:/…"
    let is_win_abs = norm.len() >= 3
        && norm.as_bytes()[0].is_ascii_alphabetic()
        && norm.as_bytes()[1] == b':'
        && norm.as_bytes()[2] == b'/';
    // Unix absolute: "/…"
    let is_unix_abs = norm.starts_with('/');

    let full_path = if is_win_abs || is_unix_abs {
        norm
    } else {
        // Relative — prepend base URI directory.
        let base_path = base_uri.path();
        let dir = match base_path.rfind('/') {
            Some(pos) => &base_path[..=pos],
            None => "/",
        };
        format!("{}{}", dir, norm)
    };

    // Normalise the combined path string (collapse //, resolve . and ..).
    let normalised = normalize_path(&full_path);

    // Build the file:// URL from the normalised path.
    let url = Url::parse(&format!("file://{}", normalised)).ok()?;

    // Check whether the file actually exists on disk.
    let exists = url
        .to_file_path()
        .map(|p| p.exists())
        .unwrap_or(false);

    Some(ResolvedImport { url, exists })
}

/// Collapse consecutive `/`, resolve `.` and `..` in a forward-slash path.
///
/// `..` past the root (or drive prefix) is silently ignored — the OS is the
/// ultimate authority, but for URL construction we can't go higher.
///
/// Examples:
/// * `/a/b/c/../../../a` → `/a`
/// * `/a/b/c/../../../../x` → `/x`
/// * `C:/a/../b` → (url-path) `/C:/b`
fn normalize_path(path: &str) -> String {
    // Separate optional Windows drive prefix ("C:" etc.)
    let (prefix, rest) = if path.len() >= 2
        && path.as_bytes()[0].is_ascii_alphabetic()
        && path.as_bytes()[1] == b':'
    {
        (&path[..2], &path[2..])
    } else if path.len() >= 4
        && path.as_bytes()[0] == b'/'
        && path.as_bytes()[1].is_ascii_alphabetic()
        && path.as_bytes()[2] == b':'
        && path.as_bytes()[3] == b'/'
    {
        // Already has leading / before drive letter (from base_uri.path()).
        (&path[1..3], &path[3..])
    } else {
        ("", path)
    };

    let mut parts: Vec<&str> = Vec::new();
    for seg in rest.split('/') {
        match seg {
            "" | "." => continue,
            ".." => { parts.pop(); }
            _ => parts.push(seg),
        }
    }

    if prefix.is_empty() {
        // Unix absolute — leading /
        format!("/{}", parts.join("/"))
    } else {
        // Windows drive — e.g. /C:/dir/file
        format!("/{}/{}", prefix, parts.join("/"))
    }
}

