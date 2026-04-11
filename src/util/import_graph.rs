use log::{error, info};
use once_cell::sync::Lazy;
use petgraph::algo::is_cyclic_directed;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::{Bfs, EdgeRef};
use petgraph::Direction;
use redb::{ReadableDatabase, ReadableTable};
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;
use url::Url;

use crate::util::cache_db;

// ─── ImportGraph ─────────────────────────────────────────────────────────────

/// Global import graph shared by all languages.
pub static IMPORT_GRAPH: Lazy<ImportGraph> = Lazy::new(ImportGraph::load);

/// Directed import graph backed by `petgraph`.
///
/// Each node is a `Url`. An edge `A → B` means "A imports B".
///
/// Persisted in the `import_graph` table of the shared `redb` database.
pub struct ImportGraph {
    inner: RwLock<GraphInner>,
    /// Cached entry-point mapping: file URI → set of entry URIs that can
    /// reach it via outgoing edges.  Recomputed by [`recompute_entry_cache`]
    /// after every import-graph mutation so that callers don't need to BFS
    /// each time.
    entry_cache: RwLock<HashMap<Url, HashSet<Url>>>,
    /// URIs that are imported via `//import!` (frozen) by at least one file.
    /// Updated eagerly by [`mark_frozen`] so that [`tree_for_uri`] can prune
    /// incoming edges without waiting for PARSE_CACHE snapshots.
    frozen_targets: RwLock<HashSet<Url>>,
    /// Persistent set of URIs that contain `//entry` directive.
    /// Survives server restarts (persisted in redb alongside graph edges).
    /// Updated by [`mark_entry`]; consumed by [`recompute_entry_cache`].
    entry_nodes: RwLock<HashSet<Url>>,
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
            frozen_targets: RwLock::new(HashSet::new()),
            entry_nodes: RwLock::new(HashSet::new()),
        }
    }

    /// Load from the redb database, or create empty.
    fn load() -> Self {
        let mut inner = GraphInner::new();
        let mut loaded_entry_nodes: HashSet<Url> = HashSet::new();

        if let Some(db) = cache_db::db() {
            if let Ok(read_txn) = db.begin_read() {
                if let Ok(table) = read_txn.open_table(cache_db::IMPORT_TABLE) {
                    if let Ok(iter) = table.iter() {
                        for entry_result in iter {
                            let (key_guard, val_guard): (redb::AccessGuard<&str>, redb::AccessGuard<&[u8]>) = match entry_result {
                                Ok(kv) => kv,
                                Err(_) => continue,
                            };
                            let from_str: &str = key_guard.value();
                            let bytes: &[u8] = val_guard.value();

                            // Special key: persistent entry-point URIs.
                            if from_str == "__entries__" {
                                if let Ok(entries) = bitcode::deserialize::<Vec<Url>>(bytes) {
                                    loaded_entry_nodes = entries.into_iter().collect();
                                }
                                continue;
                            }

                            let from = match Url::parse(from_str) {
                                Ok(u) => u,
                                Err(_) => continue,
                            };
                            let tos: Vec<Url> = match bitcode::deserialize(bytes) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };

                            let from_idx = inner.ensure_node(&from);
                            for to in &tos {
                                let to_idx = inner.ensure_node(to);
                                inner.graph.update_edge(from_idx, to_idx, ());
                            }
                        }

                        info!(
                            "import_graph: loaded {} files, {} edges, {} entry points from redb",
                            inner.graph.node_count(),
                            inner.graph.edge_count(),
                            loaded_entry_nodes.len(),
                        );
                    }
                }
            }
        }

        Self {
            inner: RwLock::new(inner),
            entry_cache: RwLock::new(HashMap::new()),
            frozen_targets: RwLock::new(HashSet::new()),
            entry_nodes: RwLock::new(loaded_entry_nodes),
        }
    }

    /// Persist current state to the redb database.
    fn save(inner: &GraphInner, entry_nodes: &HashSet<Url>) {
        let Some(db) = cache_db::db() else { return };

        let write_txn = match db.begin_write() {
            Ok(t) => t,
            Err(e) => {
                error!("import_graph: begin_write: {}", e);
                return;
            }
        };

        // Delete the whole table and recreate it with current state.
        // This is simpler and safer than trying to diff per-node changes.
        let _ = write_txn.delete_table(cache_db::IMPORT_TABLE);

        {
            let mut table = match write_txn.open_table(cache_db::IMPORT_TABLE) {
                Ok(t) => t,
                Err(e) => {
                    error!("import_graph: open table: {}", e);
                    return;
                }
            };

            for idx in inner.graph.node_indices() {
                let from = &inner.graph[idx];
                let tos: Vec<Url> = inner
                    .graph
                    .neighbors_directed(idx, Direction::Outgoing)
                    .map(|n| inner.graph[n].clone())
                    .collect();

                if !tos.is_empty() {
                    if let Ok(data) = bitcode::serialize(&tos) {
                        let _ = table.insert(from.as_str(), data.as_slice());
                    }
                }
            }

            // Persist entry-point URIs as a special key.
            if !entry_nodes.is_empty() {
                let entry_vec: Vec<Url> = entry_nodes.iter().cloned().collect();
                if let Ok(data) = bitcode::serialize(&entry_vec) {
                    let _ = table.insert("__entries__", data.as_slice());
                }
            }
        }

        if let Err(e) = write_txn.commit() {
            error!("import_graph: commit: {}", e);
        }
    }

    // ─── Mutation ────────────────────────────────────────────────────────

    /// Add `uri` as a node in the graph if it doesn't already exist.
    ///
    /// Called when a file is first opened by the editor so that it appears
    /// in the graph even if nothing imports it and it imports nothing.
    pub fn ensure_node_pub(&self, uri: &Url) {
        let mut inner = self.inner.write().unwrap();
        if inner.index.contains_key(uri) {
            return;
        }
        inner.ensure_node(uri);
        Self::save(&inner, &self.entry_nodes.read().unwrap());
    }

    /// `true` when the graph already has a node for `uri`.
    pub fn contains(&self, uri: &Url) -> bool {
        self.inner.read().unwrap().index.contains_key(uri)
    }

    /// Update the import list for `uri`.
    ///
    /// Register `targets` as frozen (imported via `//import!`).
    ///
    /// Called eagerly during parse — before `visible_component` — so that
    /// `tree_for_uri` can prune incoming edges of frozen nodes even when
    /// PARSE_CACHE doesn't have the importer's snapshot yet.
    pub fn mark_frozen(&self, targets: &HashSet<Url>) {
        if targets.is_empty() {
            return;
        }
        let mut ft = self.frozen_targets.write().unwrap();
        ft.extend(targets.iter().cloned());
    }

    /// Check if `uri` is frozen according to the graph's own bookkeeping.
    pub fn is_frozen(&self, uri: &Url) -> bool {
        self.frozen_targets.read().unwrap().contains(uri)
    }

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
        // Collect URIs to evict from PARSE_CACHE *after* releasing the
        // write lock — accessing PARSE_CACHE under the import-graph lock
        // inverts the lock ordering and can deadlock with
        // `build_update_response` (which reads PARSE_CACHE then
        // IMPORT_GRAPH).
        let mut evict_from_cache: Vec<Url> = Vec::new();
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
                    evict_from_cache.push(target_uri.clone());
                }
            }
        }

        Self::save(&inner, &self.entry_nodes.read().unwrap());
        drop(inner);

        // Evict outside the write lock to avoid deadlock.
        for uri in &evict_from_cache {
            crate::util::parse_cache::PARSE_CACHE.remove(uri);
        }
    }

    /// Remove a file node and all its edges (e.g. file deleted from disk).
    pub fn remove(&self, uri: &Url) {
        let mut inner = self.inner.write().unwrap();
        let need_evict = if let Some(&idx) = inner.index.get(uri) {
            inner.graph.remove_node(idx);
            inner.index.remove(uri);
            // petgraph may swap indices on remove — rebuild index
            let rebuilt: HashMap<Url, NodeIndex> = inner
                .graph
                .node_indices()
                .map(|idx| (inner.graph[idx].clone(), idx))
                .collect();
            inner.index = rebuilt;
            Self::save(&inner, &self.entry_nodes.read().unwrap());
            true
        } else {
            false
        };
        drop(inner);
        // Evict outside the write lock to avoid deadlock.
        if need_evict {
            crate::util::parse_cache::PARSE_CACHE.remove(uri);
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
                    .map(|p| !p.is_file())
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
                    removed.push(uri);
                }
                None => break,
            }
        }

        if !removed.is_empty() {
            Self::save(&inner, &self.entry_nodes.read().unwrap());
            info!("import_graph: gc removed {} node(s)", removed.len());
        }
        drop(inner);

        // Evict outside the write lock to avoid deadlock.
        for uri in &removed {
            crate::util::parse_cache::PARSE_CACHE.remove(uri);
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

        Self::save(&inner, &self.entry_nodes.read().unwrap());
        drop(inner);

        // Evict stale cache for the old URI outside the write lock.
        crate::util::parse_cache::PARSE_CACHE.remove(old_uri);
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
    /// Merges entry-point URIs from two sources:
    /// 1. **Persistent** — `entry_nodes` (loaded from redb, survives restart).
    /// 2. **In-memory** — `PARSE_CACHE` snapshots with `is_entry == true`.
    ///
    /// BFS forward from every entry-point and record which entries can reach
    /// each node.  Call this after every import-graph or `is_entry` mutation.
    pub fn recompute_entry_cache(&self) {
        // Merge persistent entry nodes with PARSE_CACHE entries.
        let mut all_entries: HashSet<Url> = self.entry_nodes.read().unwrap().clone();
        for uri in crate::util::parse_cache::entry_uris() {
            all_entries.insert(uri);
        }

        let inner = self.inner.read().unwrap();
        let mut new_cache: HashMap<Url, HashSet<Url>> = HashMap::new();

        for entry_uri in &all_entries {
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

    /// Update the persistent entry-point status for `uri`.
    ///
    /// When `is_entry` is `true`, the URI is added to the persistent set.
    /// When `false`, it is removed.  The change is persisted to redb.
    pub fn mark_entry(&self, uri: &Url, is_entry: bool) {
        let mut entries = self.entry_nodes.write().unwrap();
        let changed = if is_entry {
            entries.insert(uri.clone())
        } else {
            entries.remove(uri)
        };
        if changed {
            self.save_entry_nodes(&entries);
        }
    }

    /// Persist only the entry-node set (without rewriting the entire table).
    fn save_entry_nodes(&self, entries: &HashSet<Url>) {
        let Some(db) = cache_db::db() else { return };
        let entry_vec: Vec<Url> = entries.iter().cloned().collect();
        let data = match bitcode::serialize(&entry_vec) {
            Ok(d) => d,
            Err(_) => return,
        };
        let write_txn = match db.begin_write() {
            Ok(t) => t,
            Err(_) => return,
        };
        {
            let mut table = match write_txn.open_table(cache_db::IMPORT_TABLE) {
                Ok(t) => t,
                Err(_) => return,
            };
            let _ = table.insert("__entries__", data.as_slice());
        }
        let _ = write_txn.commit();
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

    /// Canonical tree resolution for `uri`.
    ///
    /// Returns the set of all URIs in the same logical tree as `uri`,
    /// **including** `uri` itself.
    ///
    /// Algorithm:
    /// 1. Look up `//entry` roots that own this file (from `entry_cache`).
    /// 2. If not in the cache, climb **incoming** edges upward until
    ///    `//entry` root(s) are found.
    /// 3. From found entries, collect the full tree (all files reachable
    ///    via outgoing edges from those entries).
    /// 4. If no `//entry` root is found — the file doesn't belong to any
    ///    tree; return only itself and its outgoing transitive imports.
    pub fn tree_for_uri(&self, uri: &Url) -> HashSet<Url> {
        let inner = self.inner.read().unwrap();
        let cache = self.entry_cache.read().unwrap();

        // ── Step 1: fast path — entry_cache already knows our entries ────
        let mut found_entries: HashSet<Url> = cache
            .get(uri)
            .cloned()
            .unwrap_or_default();

        // ── Step 2: climb incoming edges to discover entry roots ─────────
        if found_entries.is_empty() {
            if let Some(&start) = inner.index.get(uri) {
                let mut up_visited: HashSet<NodeIndex> = HashSet::new();
                let mut up_queue = std::collections::VecDeque::new();
                up_queue.push_back(start);
                up_visited.insert(start);

                while let Some(cur) = up_queue.pop_front() {
                    let cur_uri = &inner.graph[cur];
                    // Is this node an entry point?
                    if let Some(entries) = cache.get(cur_uri) {
                        if entries.contains(cur_uri) {
                            found_entries.insert(cur_uri.clone());
                            continue; // don't climb further from an entry
                        }
                    }
                    for next in inner.graph.neighbors_directed(cur, Direction::Incoming) {
                        if up_visited.insert(next) {
                            up_queue.push_back(next);
                        }
                    }
                }
            }
        }

        // ── Step 3: entries found → full tree from those entries ─────────
        if !found_entries.is_empty() {
            let mut result: HashSet<Url> = cache
                .iter()
                .filter(|(_, entries)| entries.iter().any(|e| found_entries.contains(e)))
                .map(|(k, _)| k.clone())
                .collect();
            result.insert(uri.clone());
            drop(cache);
            return result;
        }

        drop(cache);

        // ── Step 4: no entry → file not in any tree, outgoing deps only ──
        let Some(&start) = inner.index.get(uri) else {
            let mut s = HashSet::new();
            s.insert(uri.clone());
            return s;
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
        }

        visited.iter().map(|&n| inner.graph[n].clone()).collect()
    }

    /// Entry-point-aware visible component for `uri`.
    ///
    /// Delegates to [`tree_for_uri`] and excludes `uri` itself from the
    /// result, matching the contract expected by parse / completion / hover.
    pub fn visible_component(&self, uri: &Url) -> HashSet<Url> {
        let mut tree = self.tree_for_uri(uri);
        tree.remove(uri);
        tree
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

    /// Same as [`tree_for_uri`] but returns a **deterministically ordered**
    /// `Vec<Url>`:
    ///
    /// 1. Find the root(s) — nodes in the tree with no incoming edges from
    ///    other tree members (i.e. entry points or top-level files).
    /// 2. BFS from those roots; at every level children are sorted
    ///    alphabetically by URL string.
    /// 3. Any remaining nodes (cycles) are appended alphabetically.
    ///
    /// This guarantees that repeated calls always return the same order.
    pub fn tree_for_uri_sorted(&self, uri: &Url) -> Vec<Url> {
        let tree_set = self.tree_for_uri(uri);
        if tree_set.is_empty() {
            return vec![];
        }

        let inner = self.inner.read().unwrap();

        // Find roots: tree members with no incoming edges from within the tree.
        let mut roots: Vec<Url> = Vec::new();
        for u in &tree_set {
            if let Some(&idx) = inner.index.get(u) {
                let has_internal_incoming = inner
                    .graph
                    .neighbors_directed(idx, Direction::Incoming)
                    .any(|n| tree_set.contains(&inner.graph[n]));
                if !has_internal_incoming {
                    roots.push(u.clone());
                }
            } else {
                roots.push(u.clone());
            }
        }

        if roots.is_empty() {
            // Every node has an internal predecessor (cycle) — start from `uri`.
            roots.push(uri.clone());
        }

        roots.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        // BFS with alphabetically-sorted children at each level.
        let mut result: Vec<Url> = Vec::with_capacity(tree_set.len());
        let mut visited: HashSet<Url> = HashSet::new();
        let mut queue: std::collections::VecDeque<Url> = std::collections::VecDeque::new();

        for root in roots {
            if visited.insert(root.clone()) {
                queue.push_back(root);
            }
        }

        while let Some(current) = queue.pop_front() {
            result.push(current.clone());

            if let Some(&idx) = inner.index.get(&current) {
                let mut children: Vec<Url> = inner
                    .graph
                    .neighbors_directed(idx, Direction::Outgoing)
                    .filter(|&n| {
                        tree_set.contains(&inner.graph[n])
                            && !visited.contains(&inner.graph[n])
                    })
                    .map(|n| inner.graph[n].clone())
                    .collect();
                children.sort_by(|a, b| a.as_str().cmp(b.as_str()));
                for child in children {
                    if visited.insert(child.clone()) {
                        queue.push_back(child);
                    }
                }
            }
        }

        // Append remaining unreached nodes (e.g. cycles) alphabetically.
        let mut remaining: Vec<Url> = tree_set
            .into_iter()
            .filter(|u| !visited.contains(u))
            .collect();
        remaining.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        result.extend(remaining);

        result
    }

    /// Return the **connected subgraph** reachable from `uri`.
    ///
    /// Delegates to [`tree_for_uri`] for node collection, so the import
    /// graph panel shows exactly the same set of files as dependency
    /// resolution / completion / diagnostics.
    ///
    /// The result is a pair `(nodes, edges)` where each node is a URL string
    /// and each edge is `(source_index, target_index)` into the nodes vec.
    /// `nodes[0]` is always `uri` itself (when it exists in the graph).
    pub fn subgraph_for(&self, uri: &Url) -> (Vec<String>, Vec<(usize, usize)>) {
        let tree_uris = self.tree_for_uri(uri);

        let inner = self.inner.read().unwrap();
        let Some(&start) = inner.index.get(uri) else {
            return (vec![uri.to_string()], vec![]);
        };

        // Collect NodeIndex set for the tree URIs.
        let visited: HashSet<NodeIndex> = tree_uris
            .iter()
            .filter_map(|u| inner.index.get(u).copied())
            .collect();

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

    // Check whether the file actually exists on disk (must be a regular file,
    // not a directory).
    let exists = url
        .to_file_path()
        .map(|p| p.is_file())
        .unwrap_or(false);

    Some(ResolvedImport { url, exists })
}



