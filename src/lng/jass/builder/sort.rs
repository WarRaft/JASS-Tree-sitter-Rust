//! Topological sort of JASS functions by call-graph order.
//!
//! `config` is placed first, `main` is placed last; all other functions
//! are ordered so that every callee appears before its caller (DFS post-order).

use std::collections::{HashMap, HashSet};

/// Sort `functions` (name → source text) in topological order.
///
/// * `order_hint` — names in declaration/import order (tie-breaker)
/// * `callees`   — name → set of names called by that function
///
/// Returns the sorted list of names.  Names absent from `functions` are skipped.
pub fn topo_sort(
    order_hint: &[String],
    functions: &HashMap<String, String>,
    callees: &HashMap<String, HashSet<String>>,
) -> Vec<String> {
    fn dfs(
        name: &str,
        functions: &HashMap<String, String>,
        callees: &HashMap<String, HashSet<String>>,
        visited: &mut HashSet<String>,
        out: &mut Vec<String>,
    ) {
        if visited.contains(name) {
            return;
        }
        visited.insert(name.to_string());

        if let Some(cs) = callees.get(name) {
            let mut cs_sorted: Vec<&String> = cs.iter().collect();
            cs_sorted.sort();
            for callee in cs_sorted {
                if functions.contains_key(callee) {
                    dfs(callee, functions, callees, visited, out);
                }
            }
        }

        out.push(name.to_string());
    }

    let mut visited = HashSet::new();
    let mut out = Vec::new();

    for name in order_hint {
        if functions.contains_key(name) {
            dfs(name, functions, callees, &mut visited, &mut out);
        }
    }

    // Any functions not reached via order_hint (shouldn't happen normally).
    let mut extra: Vec<&String> = functions.keys().filter(|n| !visited.contains(*n)).collect();
    extra.sort();
    for name in extra {
        dfs(name, functions, callees, &mut visited, &mut out);
    }

    // `config` first, `main` last — JASS engine convention.
    if let Some(pos) = out.iter().position(|n| n == "config") {
        let cfg = out.remove(pos);
        out.insert(0, cfg);
    }
    if let Some(pos) = out.iter().position(|n| n == "main") {
        let main = out.remove(pos);
        out.push(main);
    }

    out
}

