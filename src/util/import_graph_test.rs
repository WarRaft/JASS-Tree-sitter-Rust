#[cfg(test)]
mod tests {
    use crate::util::import_graph::{ImportGraph, resolve_import};
    use std::collections::HashSet;
    use url::Url;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn new_graph() -> ImportGraph {
        ImportGraph::new_empty()
    }

    #[test]
    fn update_and_direct() {
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");

        g.update(&a, HashSet::from([b.clone(), c.clone()]));

        let imports = g.direct_imports(&a);
        assert_eq!(imports.len(), 2);
        assert!(imports.contains(&b));
        assert!(imports.contains(&c));
        assert_eq!(g.direct_dependents(&b), vec![a.clone()]);
        assert_eq!(g.direct_dependents(&c), vec![a.clone()]);
    }

    #[test]
    fn update_diff_removes_old() {
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");

        g.update(&a, HashSet::from([b.clone(), c.clone()]));
        g.update(&a, HashSet::from([c.clone()]));

        assert_eq!(g.direct_imports(&a), vec![c.clone()]);
        assert!(g.direct_dependents(&b).is_empty());
        assert_eq!(g.direct_dependents(&c), vec![a.clone()]);
    }

    #[test]
    fn remove_cleans_all() {
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");

        g.update(&a, HashSet::from([b.clone()]));
        g.remove(&a);

        assert!(g.direct_imports(&a).is_empty());
        assert!(g.direct_dependents(&b).is_empty());
    }

    #[test]
    fn transitive_dependents() {
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");

        g.update(&a, HashSet::from([b.clone()]));
        g.update(&b, HashSet::from([c.clone()]));

        let deps = g.dependents(&c);
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&a));
        assert!(deps.contains(&b));
    }

    #[test]
    fn transitive_dependencies() {
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");

        g.update(&a, HashSet::from([b.clone()]));
        g.update(&b, HashSet::from([c.clone()]));

        let deps = g.dependencies(&a);
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&b));
        assert!(deps.contains(&c));
    }

    #[test]
    fn cycle_detection() {
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");

        g.update(&a, HashSet::from([b.clone()]));
        assert!(!g.has_cycle());

        g.update(&b, HashSet::from([a.clone()]));
        assert!(g.has_cycle());

        let deps = g.dependents(&a);
        assert!(deps.contains(&b));
    }

    #[test]
    fn cycles_for_finds_cycle() {
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");

        g.update(&a, HashSet::from([b.clone()]));
        g.update(&b, HashSet::from([c.clone()]));
        g.update(&c, HashSet::from([a.clone()]));

        let cycles = g.cycles_for(&a);
        assert!(!cycles.is_empty());
        assert_eq!(cycles[0].len(), 3);
    }

    #[test]
    fn toposort_no_cycle() {
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");

        g.update(&a, HashSet::from([b.clone()]));
        g.update(&b, HashSet::from([c.clone()]));

        let sorted = g.toposort().unwrap();
        assert_eq!(sorted.len(), 3);
        let pos_a = sorted.iter().position(|x| *x == a).unwrap();
        let pos_b = sorted.iter().position(|x| *x == b).unwrap();
        let pos_c = sorted.iter().position(|x| *x == c).unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn toposort_with_cycle_returns_none() {
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");

        g.update(&a, HashSet::from([b.clone()]));
        g.update(&b, HashSet::from([a.clone()]));

        assert!(g.toposort().is_none());
    }

    #[test]
    fn edges_bitcode_roundtrip() {
        let edges: Vec<Url> = vec![u("file:///b.j"), u("file:///c.j")];
        let encoded = bitcode::serialize(&edges).unwrap();
        let decoded: Vec<Url> = bitcode::deserialize(&encoded).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0], u("file:///b.j"));
        assert_eq!(decoded[1], u("file:///c.j"));
    }

    #[test]
    fn node_and_edge_counts() {
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");

        g.update(&a, HashSet::from([b.clone(), c.clone()]));
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn resolve_import_relative() {
        let base = u("file:///project/src/main.j");
        let resolved = resolve_import(&base, "\"utils/helper.j\"").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///project/src/utils/helper.j");
    }

    #[test]
    fn resolve_import_parent() {
        let base = u("file:///project/src/sub/main.j");
        let resolved = resolve_import(&base, "../common.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///project/src/common.j");
    }

    #[test]
    fn resolve_import_empty() {
        let base = u("file:///project/src/main.j");
        assert!(resolve_import(&base, "").is_none());
        assert!(resolve_import(&base, "\"\"").is_none());
    }

    #[test]
    fn resolve_import_backslashes() {
        let base = u("file:///project/src/main.j");
        let resolved = resolve_import(&base, r"utils\helper.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///project/src/utils/helper.j");
    }

    #[test]
    fn resolve_import_mixed_slashes() {
        let base = u("file:///project/src/main.j");
        let resolved = resolve_import(&base, r"utils/sub\file.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///project/src/utils/sub/file.j");
    }

    #[test]
    fn resolve_import_dot_current() {
        let base = u("file:///project/src/main.j");
        let resolved = resolve_import(&base, "./helper.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///project/src/helper.j");
    }

    #[test]
    fn resolve_import_dot_backslash() {
        let base = u("file:///project/src/main.j");
        let resolved = resolve_import(&base, r".\helper.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///project/src/helper.j");
    }

    #[test]
    fn resolve_import_parent_backslash() {
        let base = u("file:///project/src/sub/main.j");
        let resolved = resolve_import(&base, r"..\common.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///project/src/common.j");
    }

    #[test]
    fn resolve_import_unix_absolute() {
        let base = u("file:///project/src/main.j");
        let resolved = resolve_import(&base, "/usr/share/jass/common.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///usr/share/jass/common.j");
    }

    #[test]
    fn resolve_import_unix_absolute_with_dotdot() {
        let base = u("file:///project/src/main.j");
        let resolved = resolve_import(&base, "/usr/share/../lib/common.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///usr/lib/common.j");
    }

    #[test]
    fn resolve_import_win_drive_forward() {
        let base = u("file:///D:/project/main.j");
        let resolved = resolve_import(&base, "C:/jass/common.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///C:/jass/common.j");
    }

    #[test]
    fn resolve_import_win_drive_backslash() {
        let base = u("file:///D:/project/main.j");
        let resolved = resolve_import(&base, r"C:\jass\common.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///C:/jass/common.j");
    }

    #[test]
    fn resolve_import_win_drive_double_slash() {
        let base = u("file:///D:/project/main.j");
        let resolved = resolve_import(&base, "C://jass//common.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///C:/jass/common.j");
    }

    #[test]
    fn resolve_import_win_drive_double_backslash() {
        let base = u("file:///D:/project/main.j");
        let resolved = resolve_import(&base, r"C:\\jass\\common.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///C:/jass/common.j");
    }

    #[test]
    fn resolve_import_win_drive_with_dotdot() {
        let base = u("file:///D:/project/main.j");
        let resolved = resolve_import(&base, r"C:\jass\sub\..\common.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///C:/jass/common.j");
    }

    #[test]
    fn resolve_import_consecutive_slashes() {
        let base = u("file:///project/src/main.j");
        let resolved = resolve_import(&base, "utils//helper.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///project/src/utils/helper.j");
    }

    #[test]
    fn resolve_import_wandering_collapse() {
        // /a/b/c/../../../a → pop c, pop b, pop a → /a
        let base = u("file:///x/main.j");
        let resolved = resolve_import(&base, "/a/b/c/../../../a").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///a");
    }

    #[test]
    fn resolve_import_dotdot_above_root() {
        // Extra .. beyond root are silently ignored
        let base = u("file:///x/main.j");
        let resolved = resolve_import(&base, "/a/b/c/../../../../x").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///x");
    }

    #[test]
    fn resolve_import_relative_dotdot_above_root() {
        // base dir = /project/src/, path = ../../../x.j
        // /project/src + .. = /project, + .. = /, + .. = / (clamped), + x.j = /x.j
        let base = u("file:///project/src/main.j");
        let resolved = resolve_import(&base, "../../../x.j").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///x.j");
    }

    #[test]
    fn resolve_import_win_drive_dotdot_above_root() {
        // C:/a/../../b → C:/b  (.. can't escape the drive)
        let base = u("file:///D:/x.j");
        let resolved = resolve_import(&base, r"C:\a\..\..\b").unwrap();
        assert_eq!(resolved.url.as_str(), "file:///C:/b");
    }

    #[test]
    fn resolve_import_nonexistent_file() {
        let base = u("file:///project/src/main.j");
        let resolved = resolve_import(&base, "does_not_exist_12345.j").unwrap();
        assert!(!resolved.exists);
    }

    #[test]
    fn resolve_import_existing_file() {
        // Cargo.toml always exists in the project root
        let cargo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let base_url = url::Url::from_file_path(cargo.parent().unwrap().join("dummy.rs")).unwrap();
        let resolved = resolve_import(&base_url, "Cargo.toml").unwrap();
        assert!(resolved.exists);
    }

    #[test]
    fn subgraph_for_basic() {
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");
        let d = u("file:///d.j"); // disconnected

        g.update(&a, HashSet::from([b.clone()]));
        g.update(&b, HashSet::from([c.clone()]));
        g.update(&d, HashSet::new());

        let (nodes, edges) = g.subgraph_for(&b);
        // Without //entry, tree_for_uri returns only outgoing deps.
        // b→c, so subgraph is {b, c}.  a (dependent of b) is not included.
        assert_eq!(nodes[0], b.to_string());
        assert_eq!(nodes.len(), 2);
        assert!(nodes.contains(&c.to_string()));
        // a and d should not appear
        assert!(!nodes.contains(&a.to_string()));
        assert!(!nodes.contains(&d.to_string()));
        // edges: b→c
        assert_eq!(edges.len(), 1);
    }

    #[test]
    fn subgraph_for_unknown_uri() {
        let g = new_graph();
        let unknown = u("file:///unknown.j");
        let (nodes, edges) = g.subgraph_for(&unknown);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0], unknown.to_string());
        assert!(edges.is_empty());
    }

    #[test]
    fn subgraph_for_isolated_node() {
        let g = new_graph();
        let a = u("file:///a.j");
        g.update(&a, HashSet::new());
        let (nodes, edges) = g.subgraph_for(&a);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0], a.to_string());
        assert!(edges.is_empty());
    }

    // ── connected_component ──────────────────────────────────────────────

    #[test]
    fn connected_component_one_way_import() {
        // A → B  (A imports B)
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        g.update(&a, HashSet::from([b.clone()]));

        // From A: component = {B}
        let ca = g.connected_component(&a);
        assert_eq!(ca.len(), 1);
        assert!(ca.contains(&b));

        // From B: component = {A}  (bidirectional reach!)
        let cb = g.connected_component(&b);
        assert_eq!(cb.len(), 1);
        assert!(cb.contains(&a));
    }

    #[test]
    fn connected_component_triangle() {
        // A → B, A → C  →  A, B, C are all connected
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");
        g.update(&a, HashSet::from([b.clone(), c.clone()]));

        // From B: should see {A, C}
        let cb = g.connected_component(&b);
        assert_eq!(cb.len(), 2);
        assert!(cb.contains(&a));
        assert!(cb.contains(&c));

        // From C: should see {A, B}
        let cc = g.connected_component(&c);
        assert_eq!(cc.len(), 2);
        assert!(cc.contains(&a));
        assert!(cc.contains(&b));
    }

    #[test]
    fn connected_component_chain() {
        // A → B → C  →  all three connected
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");
        g.update(&a, HashSet::from([b.clone()]));
        g.update(&b, HashSet::from([c.clone()]));

        // From C: should see {A, B}
        let cc = g.connected_component(&c);
        assert_eq!(cc.len(), 2);
        assert!(cc.contains(&a));
        assert!(cc.contains(&b));
    }

    #[test]
    fn connected_component_separate_clusters() {
        // Cluster 1: A → B
        // Cluster 2: C → D (disconnected from A,B)
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");
        let d = u("file:///d.j");
        g.update(&a, HashSet::from([b.clone()]));
        g.update(&c, HashSet::from([d.clone()]));

        // From A: only B
        let ca = g.connected_component(&a);
        assert_eq!(ca.len(), 1);
        assert!(ca.contains(&b));
        assert!(!ca.contains(&c));
        assert!(!ca.contains(&d));

        // From D: only C
        let cd = g.connected_component(&d);
        assert_eq!(cd.len(), 1);
        assert!(cd.contains(&c));
    }

    #[test]
    fn connected_component_unknown_uri() {
        let g = new_graph();
        let unknown = u("file:///unknown.j");
        let c = g.connected_component(&unknown);
        assert!(c.is_empty());
    }

    #[test]
    fn connected_component_excludes_self() {
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        g.update(&a, HashSet::from([b.clone()]));

        let ca = g.connected_component(&a);
        assert!(!ca.contains(&a), "connected_component should exclude self");
    }

    #[test]
    fn update_removes_orphan_target() {
        // When A imports B, then A stops importing B,
        // B should be removed from the graph (orphan GC in update).
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");

        g.update(&a, HashSet::from([b.clone()]));
        assert!(g.all_uris().contains(&b));

        // A no longer imports B → B becomes orphan and is GC'd inline.
        g.update(&a, HashSet::new());
        assert!(
            !g.all_uris().contains(&b),
            "orphan B should be GC'd after update"
        );
        // A itself still exists — update only GC's removed *targets*,
        // not the source node itself.
        assert!(g.all_uris().contains(&a));
    }

    #[test]
    fn update_keeps_target_with_other_dependents() {
        // B is imported by both A and C.  Removing A→B should NOT GC B.
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");

        g.update(&a, HashSet::from([b.clone()]));
        g.update(&c, HashSet::from([b.clone()]));

        // Remove A's import of B
        g.update(&a, HashSet::new());
        assert!(
            g.all_uris().contains(&b),
            "B still has incoming edge from C, must NOT be GC'd"
        );
    }

    #[test]
    fn gc_orphans_cleans_isolated_nodes() {
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");

        g.update(&a, HashSet::from([b.clone(), c.clone()]));
        assert_eq!(g.all_uris().len(), 3);

        // A stops importing everything — b and c are GC'd inline,
        // but A itself stays as an orphan source node.
        g.update(&a, HashSet::new());
        assert_eq!(g.all_uris().len(), 1);
        assert!(g.all_uris().contains(&a));

        // gc_orphans should collect the remaining orphan A.
        let removed = g.gc_orphans();
        assert_eq!(removed.len(), 1);
        assert!(removed.contains(&a));
        assert!(g.all_uris().is_empty());
    }

    #[test]
    fn gc_orphans_removes_dead_file_cycle() {
        // b.j and c.j form a cycle but neither exists on disk.
        // gc_orphans should remove both even though they have edges.
        let g = new_graph();
        let b = u("file:///nonexistent_gc_test_b.j");
        let c = u("file:///nonexistent_gc_test_c.j");

        g.update(&b, HashSet::from([c.clone()]));
        g.update(&c, HashSet::from([b.clone()]));
        assert_eq!(g.all_uris().len(), 2);

        let removed = g.gc_orphans();
        assert_eq!(removed.len(), 2, "both dead cyclic nodes should be GC'd");
        assert!(g.all_uris().is_empty());
    }

    // ── tree_for_uri ─────────────────────────────────────────────────────

    #[test]
    fn tree_for_uri_no_entry_returns_outgoing_only() {
        // Without //entry, tree = self + outgoing transitive deps.
        // A → B → C.  tree_for_uri(B) = {B, C}, NOT {A, B, C}.
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");
        g.update(&a, HashSet::from([b.clone()]));
        g.update(&b, HashSet::from([c.clone()]));

        let tree = g.tree_for_uri(&b);
        assert!(tree.contains(&b));
        assert!(tree.contains(&c));
        assert!(!tree.contains(&a), "without entry, dependents should not be included");
    }

    #[test]
    fn tree_for_uri_entry_root_returns_full_tree() {
        // A(entry) → B → C.  tree_for_uri(A) = {A, B, C}.
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");
        g.update(&a, HashSet::from([b.clone()]));
        g.update(&b, HashSet::from([c.clone()]));
        g.mark_entry(&a, true);
        g.recompute_entry_cache();

        let tree = g.tree_for_uri(&a);
        assert_eq!(tree.len(), 3);
        assert!(tree.contains(&a));
        assert!(tree.contains(&b));
        assert!(tree.contains(&c));
    }

    #[test]
    fn tree_for_uri_leaf_climbs_to_entry() {
        // A(entry) → B → C.  tree_for_uri(C) should climb to A
        // and return the full tree {A, B, C}.
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");
        g.update(&a, HashSet::from([b.clone()]));
        g.update(&b, HashSet::from([c.clone()]));
        g.mark_entry(&a, true);
        g.recompute_entry_cache();

        let tree = g.tree_for_uri(&c);
        assert_eq!(tree.len(), 3);
        assert!(tree.contains(&a));
        assert!(tree.contains(&b));
        assert!(tree.contains(&c));
    }

    #[test]
    fn tree_for_uri_middle_node_climbs_to_entry() {
        // A(entry) → B → C.  tree_for_uri(B) = {A, B, C}.
        let g = new_graph();
        let a = u("file:///a.j");
        let b = u("file:///b.j");
        let c = u("file:///c.j");
        g.update(&a, HashSet::from([b.clone()]));
        g.update(&b, HashSet::from([c.clone()]));
        g.mark_entry(&a, true);
        g.recompute_entry_cache();

        let tree = g.tree_for_uri(&b);
        assert_eq!(tree.len(), 3);
        assert!(tree.contains(&a));
        assert!(tree.contains(&b));
        assert!(tree.contains(&c));
    }

    #[test]
    fn tree_for_uri_shared_file_belongs_to_multiple_trees() {
        // Two entries share a common library file:
        //   E1(entry) → shared
        //   E2(entry) → shared
        //   E1 → leaf1
        //   E2 → leaf2
        //
        // tree_for_uri(shared) should return the union of BOTH trees:
        //   {E1, E2, shared, leaf1, leaf2}
        let g = new_graph();
        let e1 = u("file:///e1.j");
        let e2 = u("file:///e2.j");
        let shared = u("file:///shared.j");
        let leaf1 = u("file:///leaf1.j");
        let leaf2 = u("file:///leaf2.j");

        g.update(&e1, HashSet::from([shared.clone(), leaf1.clone()]));
        g.update(&e2, HashSet::from([shared.clone(), leaf2.clone()]));
        g.mark_entry(&e1, true);
        g.mark_entry(&e2, true);
        g.recompute_entry_cache();

        let tree = g.tree_for_uri(&shared);
        assert_eq!(tree.len(), 5, "shared file should belong to both trees");
        assert!(tree.contains(&e1));
        assert!(tree.contains(&e2));
        assert!(tree.contains(&shared));
        assert!(tree.contains(&leaf1));
        assert!(tree.contains(&leaf2));
    }

    #[test]
    fn tree_for_uri_separate_entries_separate_trees() {
        // E1(entry) → A
        // E2(entry) → B
        // tree_for_uri(A) = {E1, A}, not {E1, E2, A, B}
        let g = new_graph();
        let e1 = u("file:///e1.j");
        let e2 = u("file:///e2.j");
        let a = u("file:///a.j");
        let b = u("file:///b.j");

        g.update(&e1, HashSet::from([a.clone()]));
        g.update(&e2, HashSet::from([b.clone()]));
        g.mark_entry(&e1, true);
        g.mark_entry(&e2, true);
        g.recompute_entry_cache();

        let tree_a = g.tree_for_uri(&a);
        assert_eq!(tree_a.len(), 2);
        assert!(tree_a.contains(&e1));
        assert!(tree_a.contains(&a));
        assert!(!tree_a.contains(&e2));
        assert!(!tree_a.contains(&b));

        let tree_b = g.tree_for_uri(&b);
        assert_eq!(tree_b.len(), 2);
        assert!(tree_b.contains(&e2));
        assert!(tree_b.contains(&b));
    }

    #[test]
    fn tree_for_uri_disconnected_from_entry() {
        // E1(entry) → A.  D is disconnected.
        // tree_for_uri(D) = {D} only (not part of any tree).
        let g = new_graph();
        let e1 = u("file:///e1.j");
        let a = u("file:///a.j");
        let d = u("file:///d.j");

        g.update(&e1, HashSet::from([a.clone()]));
        g.update(&d, HashSet::new());
        g.mark_entry(&e1, true);
        g.recompute_entry_cache();

        let tree = g.tree_for_uri(&d);
        assert_eq!(tree.len(), 1);
        assert!(tree.contains(&d));
    }
}
