#[cfg(test)]
mod tests {
    use crate::util::import_graph::{ImportGraph, Snapshot, resolve_import};
    use std::collections::{HashMap, HashSet};
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
    fn snapshot_roundtrip() {
        let snap = Snapshot {
            edges: HashMap::from([(
                u("file:///a.j"),
                vec![u("file:///b.j"), u("file:///c.j")],
            )]),
        };
        let json = serde_json::to_string(&snap).unwrap();
        let restored: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.edges.len(), 1);
        assert_eq!(restored.edges[&u("file:///a.j")].len(), 2);
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
}

