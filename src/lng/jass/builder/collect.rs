//! Collect the ordered list of files participating in the build.
//!
//! Starts from the `//entry` file that owns the given build setting key,
//! then returns all its dependencies in BFS/import order (dependencies first,
//! entry file last) with duplicates removed.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use url::Url;

use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::parse_cache::peek_or_load;

// ─── Entry-point resolution ───────────────────────────────────────────────────

/// Search for a build setting `key` (e.g. `"build-jass"`) in `//entry` files
/// within the connected component of the import graph reachable from `uri`.
///
/// Returns `(entry_uri, setting_value)` for the first matching entry file.
/// Non-entry files that carry the setting are intentionally ignored.
pub fn find_build_setting(uri: &Url, key: &str) -> Option<(Url, String)> {
    // Check the current file first.
    if let Some(snap) = peek_or_load(uri) {
        if snap.file_symbols.is_entry {
            if let Some(v) = snap.file_symbols.file_settings.get(key) {
                return Some((uri.clone(), v.clone()));
            }
        }
    }
    // Search the visible component (entry-point-aware).
    for u in &IMPORT_GRAPH.visible_component(uri) {
        if let Some(snap) = peek_or_load(u) {
            if snap.file_symbols.is_entry {
                if let Some(v) = snap.file_symbols.file_settings.get(key) {
                    return Some((u.clone(), v.clone()));
                }
            }
        }
    }
    None
}

/// Returns `true` when any file in the connected component carries the setting.
pub fn has_build_setting(uri: &Url, key: &str) -> bool {
    find_build_setting(uri, key).is_some()
}

// ─── File ordering ────────────────────────────────────────────────────────────

/// Return the ordered list of files for the build starting at `entry_uri`.
///
/// Dependencies come first (BFS traversal), the entry file itself last.
/// Duplicates are removed, preserving first-occurrence order.
pub fn collect_file_order(entry_uri: &Url) -> Vec<Url> {
    let mut deps = IMPORT_GRAPH.dependencies(entry_uri);
    deps.push(entry_uri.clone());

    let mut seen = HashSet::new();
    deps.retain(|u| seen.insert(u.clone()));
    deps
}

// ─── Source reading ───────────────────────────────────────────────────────────

/// Read the source text of `uri`.
///
/// Prefers the in-editor rope buffer (unsaved edits); falls back to disk.
pub fn read_source(uri: &Url) -> Option<String> {
    use crate::util::roper::uri_map::ROPE_MAP;
    if let Some(rope) = ROPE_MAP.get(uri) {
        return Some(rope.to_string());
    }
    let path = uri.to_file_path().ok()?;
    std::fs::read_to_string(&path).ok()
}

/// Resolve `<path>` relative to `base_dir`. If `<path>` looks like a directory
/// (ends with `/` or has no extension), append `default_file`.
pub fn resolve_output_path(base_dir: &Path, target: &str, default_file: &str) -> PathBuf {
    let raw = target.replace('\\', "/");
    let p = Path::new(&raw);

    let resolved = if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    };

    if raw.ends_with('/') || resolved.extension().is_none() {
        resolved.join(default_file)
    } else {
        resolved
    }
}

/// Parse build option tags from file settings.
///
/// Current tags:
/// - `uglify`
/// - `nolocal`
///
/// Legacy compatibility:
/// - `//set build-uglify 1` implies the `uglify` tag.
pub fn build_opt_tags(settings: &HashMap<String, String>) -> HashSet<String> {
    let mut tags = HashSet::new();

    if let Some(raw) = settings.get("build-opts") {
        for tag in raw.split_whitespace() {
            if !tag.is_empty() {
                tags.insert(tag.to_string());
            }
        }
    }

    if settings.get("build-uglify").map(|v| v == "1").unwrap_or(false) {
        tags.insert("uglify".to_string());
    }

    tags
}

pub fn has_build_opt(settings: &HashMap<String, String>, tag: &str) -> bool {
    build_opt_tags(settings).contains(tag)
}

/// Resolve `{{variable}}` hook commands for the build entry of `uri`.
///
/// Returns `(before_cmd, after_cmd, cwd)` with all template variables expanded.
pub fn resolve_hooks(uri: &Url) -> (String, String, String) {
    let trigger_uri = find_build_setting(uri, "build-jass")
        .or_else(|| find_build_setting(uri, "build-as"))
        .map(|(u, _)| u);

    let (entry_path, cwd) = match &trigger_uri {
        Some(u) => match u.to_file_path() {
            Ok(p) => {
                let dir = p.parent().unwrap_or(Path::new(".")).to_path_buf();
                (p, dir)
            }
            Err(_) => return (String::new(), String::new(), String::new()),
        },
        None => return (String::new(), String::new(), String::new()),
    };

    let before = find_build_setting(uri, "build-before")
        .filter(|(_, v)| !v.is_empty())
        .map(|(_, v)| expand_template_vars(uri, &v, &entry_path, &cwd))
        .unwrap_or_default();

    let after = find_build_setting(uri, "build-after")
        .filter(|(_, v)| !v.is_empty())
        .map(|(_, v)| expand_template_vars(uri, &v, &entry_path, &cwd))
        .unwrap_or_default();

    let cwd_str = match cwd.canonicalize() {
        Ok(c) => c.to_string_lossy().into_owned(),
        Err(_) => cwd.to_string_lossy().into_owned(),
    };

    (before, after, cwd_str)
}

fn expand_template_vars(
    uri: &Url,
    cmd: &str,
    entry_path: &Path,
    entry_dir: &Path,
) -> String {
    let norm = |p: &Path| -> String {
        match p.canonicalize() {
            Ok(c) => c.to_string_lossy().into_owned(),
            Err(_) => p.to_string_lossy().into_owned(),
        }
    };

    let entry_str = norm(entry_path);
    let entry_dir_str = norm(entry_dir);

    let target_jass_str = find_build_setting(uri, "build-jass")
        .map(|(u, v)| {
            let base = u
                .to_file_path()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                .unwrap_or_else(|| entry_dir.to_path_buf());
            norm(&resolve_output_path(&base, &v, "war3map.j"))
        })
        .unwrap_or_default();

    let target_as_str = find_build_setting(uri, "build-as")
        .map(|(u, v)| {
            let base = u
                .to_file_path()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                .unwrap_or_else(|| entry_dir.to_path_buf());
            norm(&resolve_output_path(&base, &v, "war3map.as"))
        })
        .unwrap_or_default();

    cmd.replace("{{entry-dir}}", &entry_dir_str)
        .replace("{{entry}}", &entry_str)
        .replace("{{target-jass}}", &target_jass_str)
        .replace("{{target-as}}", &target_as_str)
}

