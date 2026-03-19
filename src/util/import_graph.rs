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
    /// Cached entry-point mapping: file URI → set of entry URIs that can
    /// reach it via outgoing edges.  Recomputed by [`recompute_entry_cache`]
    /// after every import-graph mutation so that callers don't need to BFS
    /// each time.
    entry_cache: RwLock<HashMap<Url, HashSet<Url>>>,
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

#[allow(dead_code)]
impl ImportGraph {
    /// Create an empty in-memory graph (for tests).
    #[cfg(test)]
    pub(crate) fn new_empty() -> Self {
        Self {
            inner: RwLock::new(GraphInner::new()),
            entry_cache: RwLock::new(HashMap::new()),
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
            entry_cache: RwLock::new(HashMap::new()),
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

        // Targets that are being removed — candidates for GC.
        let removed_targets: Vec<Url> = old.difference(&new_imports).cloned().collect();

        inner.clear_outgoing(node);
        for imp in &new_imports {
            let to = inner.ensure_node(imp);
            inner.graph.update_edge(node, to, ());
        }

        // Garbage-collect removed targets that are now isolated
        // (zero in-degree AND zero out-degree).
        for target_uri in &removed_targets {
            if let Some(&idx) = inner.index.get(target_uri) {
                let in_deg = inner
                    .graph
                    .neighbors_directed(idx, Direction::Incoming)
                    .count();
                let out_deg = inner
                    .graph
                    .neighbors_directed(idx, Direction::Outgoing)
                    .count();
                if in_deg == 0 && out_deg == 0 {
                    inner.graph.remove_node(idx);
                    inner.index.remove(target_uri);
                    inner.index = inner
                        .graph
                        .node_indices()
                        .map(|i| (inner.graph[i].clone(), i))
                        .collect();
                    crate::util::file_store::FILE_STORE.remove(target_uri);
                }
            }
        }

        Self::save(&inner);
    }

    /// Remove a file node and all its edges (e.g. file deleted from disk).
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
            // Evict cached RefMap for the removed file.
            crate::util::file_store::FILE_STORE.remove(uri);
        }
    }

    /// Remove stale nodes from the graph.
    ///
    /// Two passes:
    /// 1. **Dead files** — nodes whose file no longer exists on disk.
    ///    Removing these first breaks cycles between deleted files
    ///    (e.g. `b.j ↔ c.j` where both were deleted).
    /// 2. **Orphans** — nodes with zero in-degree AND zero out-degree.
    ///    These are leftover phantoms after edges were removed.
    ///
    /// Returns the list of URIs that were garbage-collected.
    pub fn gc_orphans(&self) -> Vec<Url> {
        let mut inner = self.inner.write().unwrap();
        let mut removed = Vec::new();

        // ── Pass 1: remove nodes whose file doesn't exist on disk ────────
        let dead_uris: Vec<Url> = inner
            .index
            .keys()
            .filter(|uri| {
                uri.to_file_path()
                    .map(|p| !p.exists())
                    .unwrap_or(true) // non-file URI → treat as dead
            })
            .cloned()
            .collect();

        for uri in dead_uris {
            if let Some(&idx) = inner.index.get(&uri) {
                inner.graph.remove_node(idx);
                inner.index.remove(&uri);
                // petgraph swaps the last node into the removed slot —
                // rebuild the entire index to stay consistent.
                inner.index = inner
                    .graph
                    .node_indices()
                    .map(|i| (inner.graph[i].clone(), i))
                    .collect();
                crate::util::file_store::FILE_STORE.remove(&uri);
                removed.push(uri);
            }
        }

        // ── Pass 2: remove orphan nodes (zero in + zero out) ─────────────
        loop {
            let orphan = inner.graph.node_indices().find(|&idx| {
                let in_deg = inner.graph.neighbors_directed(idx, Direction::Incoming).count();
                let out_deg = inner.graph.neighbors_directed(idx, Direction::Outgoing).count();
                in_deg == 0 && out_deg == 0
            });

            match orphan {
                Some(idx) => {
                    let uri = inner.graph[idx].clone();
                    inner.graph.remove_node(idx);
                    inner.index.remove(&uri);
                    inner.index = inner
                        .graph
                        .node_indices()
                        .map(|i| (inner.graph[i].clone(), i))
                        .collect();
                    crate::util::file_store::FILE_STORE.remove(&uri);
                    removed.push(uri);
                }
                None => break,
            }
        }

        if !removed.is_empty() {
            Self::save(&inner);
            info!("import_graph: gc removed {} node(s)", removed.len());
        }
        removed
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

        // Evict stale cache for the old URI.
        crate::util::file_store::FILE_STORE.remove(old_uri);
    }

    // ─── Queries (read lock) ─────────────────────────────────────────────

    /// All files that **transitively** import `uri` (walk incoming edges).
    ///
    /// If A→B→C, then `dependents(C) = [B, A]`.
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
    pub fn has_cycle(&self) -> bool {
        let inner = self.inner.read().unwrap();
        is_cyclic_directed(&inner.graph)
    }

    /// All URIs known to the graph (for cache GC / preloading).
    pub fn all_uris(&self) -> Vec<Url> {
        let inner = self.inner.read().unwrap();
        inner.graph.node_indices().map(|n| inner.graph[n].clone()).collect()
    }

    /// Find all cycles that `uri` participates in.
    /// Returns a list of cycles, each cycle is a Vec<Url>.
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
    pub fn toposort(&self) -> Option<Vec<Url>> {
        let inner = self.inner.read().unwrap();
        petgraph::algo::toposort(&inner.graph, None)
            .ok()
            .map(|sorted| sorted.iter().map(|&n| inner.graph[n].clone()).collect())
    }

    /// Number of files in the graph.
    pub fn node_count(&self) -> usize {
        self.inner.read().unwrap().graph.node_count()
    }

    /// Number of import edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.inner.read().unwrap().graph.edge_count()
    }

    /// All URIs in the same **connected component** as `uri`, walking both
    /// outgoing and incoming edges.  The result **excludes** `uri` itself.
    ///
    /// This is the "unified scope" set: every file in the component shares
    /// the same global symbol namespace.
    pub fn connected_component(&self, uri: &Url) -> HashSet<Url> {
        let inner = self.inner.read().unwrap();
        let Some(&start) = inner.index.get(uri) else {
            return HashSet::new();
        };

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

        // Exclude the start node itself.
        visited
            .iter()
            .filter(|&&n| n != start)
            .map(|&n| inner.graph[n].clone())
            .collect()
    }

    /// Recompute the per-file entry-point cache.
    ///
    /// BFS forward from every known entry-point URI (files with `//entry`
    /// in `FILE_STORE`) and record which entries can reach each node.
    /// Call this after every import-graph or `is_entry` mutation.
    pub fn recompute_entry_cache(&self) {
        let entry_uris = crate::util::file_store::entry_uris();
        let inner = self.inner.read().unwrap();
        let mut new_cache: HashMap<Url, HashSet<Url>> = HashMap::new();

        for entry_uri in &entry_uris {
            if let Some(&start) = inner.index.get(entry_uri) {
                let mut bfs = Bfs::new(&inner.graph, start);
                while let Some(n) = bfs.next(&inner.graph) {
                    new_cache
                        .entry(inner.graph[n].clone())
                        .or_default()
                        .insert(entry_uri.clone());
                }
            }
        }

        *self.entry_cache.write().unwrap() = new_cache;
    }

    /// Return the set of entry-point URIs that can reach `uri`.
    ///
    /// Reads from the cache populated by [`recompute_entry_cache`].
    /// Returns an empty set when `uri` is not reachable from any entry.
    pub fn cached_entry_points_for(&self, uri: &Url) -> HashSet<Url> {
        self.entry_cache
            .read()
            .unwrap()
            .get(uri)
            .cloned()
            .unwrap_or_default()
    }

    /// `true` when the entry cache is non-empty (at least one `//entry`
    /// file exists and has been processed).
    pub fn has_entry_points(&self) -> bool {
        !self.entry_cache.read().unwrap().is_empty()
    }

    /// Return the set of URIs **transitively reachable** from all entry-point
    /// files (files with `//entry` directive), reading from the cached map.
    ///
    /// If no entry points exist, returns an empty set — the caller
    /// should fall back to the full connected component.
    pub fn reachable_from_entries(&self) -> HashSet<Url> {
        self.entry_cache
            .read()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    /// Return the **connected subgraph** reachable from `uri`.
    ///
    /// **Entry-point-aware traversal:** when the graph contains at least one
    /// entry point (`//entry` directive), the subgraph is computed as the set
    /// of files transitively reachable from the same entry point(s) as `uri`.
    /// This shows exactly the "build scope" of the current file.
    ///
    /// **Fallback (no entry points):** frozen-node pruning is applied instead.
    /// When a node is a frozen file (imported via `//import!` by anyone), its
    /// *incoming* edges are **not** followed.  This prevents shared library
    /// files (e.g. `common.j`) from pulling every unrelated project into the
    /// graph.  Outgoing edges of frozen nodes are still traversed so the full
    /// dependency chain is visible.
    ///
    /// The result is a pair `(nodes, edges)` where each node is a URL string
    /// and each edge is `(source_index, target_index)` into the nodes vec.
    /// `nodes[0]` is always `uri` itself (when it exists in the graph).
    pub fn subgraph_for(&self, uri: &Url) -> (Vec<String>, Vec<(usize, usize)>) {
        let inner = self.inner.read().unwrap();
        let Some(&start) = inner.index.get(uri) else {
            return (vec![uri.to_string()], vec![]);
        };

        // Read the cached entry-point map.
        let cache = self.entry_cache.read().unwrap();
        let has_entries = !cache.is_empty();

        let visited: HashSet<NodeIndex>;

        if has_entries {
            // Find which entries own `uri`.
            let my_entries: HashSet<Url> = cache
                .get(uri)
                .cloned()
                .unwrap_or_default();

            if my_entries.is_empty() {
                // File not reachable from any entry — show only its own
                // outgoing deps (and itself).
                let mut vis: HashSet<NodeIndex> = HashSet::new();
                let mut queue = std::collections::VecDeque::new();
                queue.push_back(start);
                vis.insert(start);
                while let Some(cur) = queue.pop_front() {
                    for next in inner.graph.neighbors_directed(cur, Direction::Outgoing) {
                        if vis.insert(next) {
                            queue.push_back(next);
                        }
                    }
                }
                visited = vis;
            } else {
                // Collect the union of all files reachable from those entries.
                let mut vis: HashSet<NodeIndex> = HashSet::new();
                let mut queue = std::collections::VecDeque::new();

                for eu in &my_entries {
                    if let Some(&idx) = inner.index.get(eu) {
                        if vis.insert(idx) {
                            queue.push_back(idx);
                        }
                    }
                }

                while let Some(cur) = queue.pop_front() {
                    for next in inner.graph.neighbors_directed(cur, Direction::Outgoing) {
                        if vis.insert(next) {
                            queue.push_back(next);
                        }
                    }
                }

                vis.insert(start);
                visited = vis;
            }
        } else {
            // Fallback: frozen-node pruning (original behaviour).
            let mut vis: HashSet<NodeIndex> = HashSet::new();
            let mut queue = std::collections::VecDeque::new();
            queue.push_back(start);
            vis.insert(start);

            while let Some(cur) = queue.pop_front() {
                for next in inner.graph.neighbors_directed(cur, Direction::Outgoing) {
                    if vis.insert(next) {
                        queue.push_back(next);
                    }
                }
                let cur_uri = &inner.graph[cur];
                if !crate::util::file_store::is_uri_frozen(cur_uri) {
                    for next in inner.graph.neighbors_directed(cur, Direction::Incoming) {
                        if vis.insert(next) {
                            queue.push_back(next);
                        }
                    }
                }
            }
            visited = vis;
        }

        drop(cache); // release entry_cache lock before building result

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

