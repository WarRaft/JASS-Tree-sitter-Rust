use crate::lsp::position::Position;
use crate::lsp::range::Range;
use crate::lsp::rename::lsp::{FileRename, TextEdit, WorkspaceEdit};
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::roper::uri_map::ROPE_MAP;
use log::error;
use std::collections::HashMap;
use url::Url;

/// Compute a `WorkspaceEdit` that rewrites import paths in all files that
/// import any of the renamed/moved files.
///
/// Handles two distinct cases:
///
/// **1. Dependents** — other files that import the moved file:
///   - If the original import was **absolute**, produce a new absolute path.
///   - If the original import was **relative**, produce a new relative path
///     from the dependent's directory to the new location.
///
/// **2. The moved file itself** — if a file is moved to a different directory,
///   its own **relative** imports now resolve to wrong locations. We rewrite
///   them so they point to the same targets as before the move.
///   Absolute imports inside the moved file are left unchanged.
pub fn compute_rename_edits(renames: &[FileRename]) -> WorkspaceEdit {
    let mut all_edits: HashMap<Url, Vec<TextEdit>> = HashMap::new();

    for rename in renames {
        let old_url = match Url::parse(&rename.old_uri) {
            Ok(u) => u,
            Err(_) => continue,
        };
        let new_url = match Url::parse(&rename.new_uri) {
            Ok(u) => u,
            Err(_) => continue,
        };

        // ── Part 1: Update dependents (files that import the moved file) ─
        let dependents = IMPORT_GRAPH.direct_dependents(&old_url);

        for dep_uri in &dependents {
            let text = match read_file_text(dep_uri) {
                Some(t) => t,
                None => {
                    error!("rename: could not read {}", dep_uri);
                    continue;
                }
            };

            let edits = find_import_edits_for_target(&text, dep_uri, &old_url, &new_url);

            if !edits.is_empty() {
                all_edits
                    .entry(dep_uri.clone())
                    .or_default()
                    .extend(edits);
            }
        }

        // ── Part 2: Update the moved file's own relative imports ─────────
        let old_dir_changed = dir_of_url(&old_url) != dir_of_url(&new_url);

        if old_dir_changed {
            let text = match read_file_text(&old_url) {
                Some(t) => t,
                None => {
                    error!("rename: could not read moved file {}", old_url);
                    String::new()
                }
            };

            if !text.is_empty() {
                let edits =
                    find_self_move_edits(&text, &old_url, &new_url);

                if !edits.is_empty() {
                    // The edits target the file at its *new* URI (VS Code
                    // applies workspace edits after the rename).
                    all_edits
                        .entry(new_url.clone())
                        .or_default()
                        .extend(edits);
                }
            }
        }

        // ── Update the import graph node ─────────────────────────────────
        IMPORT_GRAPH.rename_node(&old_url, &new_url);
    }

    WorkspaceEdit {
        changes: if all_edits.is_empty() {
            None
        } else {
            Some(all_edits)
        },
    }
}

/// Read the full text of a file: from ROPE_MAP if open, from disk otherwise.
fn read_file_text(uri: &Url) -> Option<String> {
    // Try open document first.
    if let Some(entry) = ROPE_MAP.get(uri) {
        let rope = entry.value();
        return Some(rope.slice_to_cow(0..rope.len()).to_string());
    }

    // Fall back to disk.
    let path = uri.to_file_path().ok()?;
    std::fs::read_to_string(path).ok()
}

/// Compute a relative path from `from_uri`'s directory to `to_uri`.
///
/// Returns forward-slash separated path, e.g. `"../lib/common.j"`.
fn relative_path(from_uri: &Url, to_uri: &Url) -> Option<String> {
    let from_path = from_uri.to_file_path().ok()?;
    let to_path = to_uri.to_file_path().ok()?;

    let from_dir = from_path.parent()?;

    // Build relative path using pathdiff.
    let rel = pathdiff_relative(from_dir, &to_path)?;

    // Normalize to forward slashes.
    Some(rel.to_string_lossy().replace('\\', "/"))
}

/// Pure-logic relative path computation (no external crate needed).
pub(crate) fn pathdiff_relative(
    base: &std::path::Path,
    target: &std::path::Path,
) -> Option<std::path::PathBuf> {
    use std::path::{Component, PathBuf};

    // Canonicalize-ish: use components to normalize.
    let base_comps: Vec<Component> = base.components().collect();
    let target_comps: Vec<Component> = target.components().collect();

    // Find common prefix length.
    let common = base_comps
        .iter()
        .zip(target_comps.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut result = PathBuf::new();

    // Go up from base.
    for _ in common..base_comps.len() {
        result.push("..");
    }

    // Go down to target.
    for comp in &target_comps[common..] {
        result.push(comp);
    }

    Some(result)
}

/// Extract the directory portion of a `file://` URL path (everything up to
/// and including the last `/`).
fn dir_of_url(url: &Url) -> String {
    let p = url.path();
    match p.rfind('/') {
        Some(pos) => p[..=pos].to_string(),
        None => "/".to_string(),
    }
}

/// Detect whether a raw import path string is absolute.
///
/// Absolute patterns:
/// * Unix: starts with `/`
/// * Windows drive letter: `C:/…`, `C:\…`
pub(crate) fn is_absolute_import(path: &str) -> bool {
    let norm = path.replace('\\', "/");
    if norm.starts_with('/') {
        return true;
    }
    // Windows drive letter: e.g. "C:/foo"
    norm.len() >= 3
        && norm.as_bytes()[0].is_ascii_alphabetic()
        && norm.as_bytes()[1] == b':'
        && norm.as_bytes()[2] == b'/'
}

/// Convert a `file://` URL to a forward-slash filesystem path string suitable
/// for use as an absolute import path.
///
/// On Windows URLs the path starts with `/C:/…` — we strip the leading `/` so
/// the result looks like `C:/dir/file.j`.  On Unix URLs the path already
/// starts with `/…` which is correct.
fn url_to_absolute_path(url: &Url) -> Option<String> {
    let p = url.path().to_string();
    // Windows: "/C:/foo" → "C:/foo"
    if p.len() >= 4
        && p.as_bytes()[0] == b'/'
        && p.as_bytes()[1].is_ascii_alphabetic()
        && p.as_bytes()[2] == b':'
        && p.as_bytes()[3] == b'/'
    {
        Some(p[1..].to_string())
    } else {
        Some(p)
    }
}

/// Compute the replacement path for one import line, preserving the
/// absolute / relative style of the original import.
///
/// * If the original `path_str` was absolute → return the new absolute path.
/// * If the original `path_str` was relative → return a new relative path
///   from `from_uri` to `target_url`.
fn replacement_path(from_uri: &Url, target_url: &Url, path_str: &str) -> Option<String> {
    if is_absolute_import(path_str) {
        url_to_absolute_path(target_url)
    } else {
        relative_path(from_uri, target_url)
    }
}

/// Scan leading `//import` / `//import!` lines in `text` (which belongs to
/// `dep_uri`) and produce `TextEdit`s that rewrite any import pointing to
/// `old_target_url` so it now points to `new_target_url`.
///
/// Preserves the import style: absolute imports stay absolute, relative
/// imports stay relative.
pub(crate) fn find_import_edits_for_target(
    text: &str,
    dep_uri: &Url,
    old_target_url: &Url,
    new_target_url: &Url,
) -> Vec<TextEdit> {
    use crate::util::import_graph::resolve_import;

    let mut edits = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();

        let (prefix, rest) = if let Some(r) = trimmed.strip_prefix("//import!") {
            ("//import!", r)
        } else if let Some(r) = trimmed.strip_prefix("//import") {
            if r.is_empty() || r.starts_with(' ') || r.starts_with('\t') {
                ("//import", r)
            } else {
                continue;
            }
        } else {
            if !trimmed.starts_with("//") {
                break;
            }
            continue;
        };

        let path_str = rest.trim();
        if path_str.is_empty() {
            continue;
        }

        let resolved = match resolve_import(dep_uri, path_str) {
            Some(r) => r,
            None => continue,
        };

        if resolved.url != *old_target_url {
            continue;
        }

        let new_path = match replacement_path(dep_uri, new_target_url, path_str) {
            Some(p) => p,
            None => continue,
        };

        let leading_ws = line.len() - trimmed.len();
        let prefix_end = leading_ws + prefix.len();
        let path_start_in_line = prefix_end + (rest.len() - rest.trim_start().len());
        let path_end_in_line = path_start_in_line + path_str.len();

        edits.push(TextEdit {
            range: Range {
                start: Position {
                    line: line_idx,
                    character: path_start_in_line,
                },
                end: Position {
                    line: line_idx,
                    character: path_end_in_line,
                },
            },
            new_text: new_path,
        });
    }

    edits
}

/// Compute edits for the moved file itself.
///
/// When a file moves to a different directory its **relative** imports break
/// because they resolve against the new directory instead of the old one.
/// This function rewrites each relative import so it points to the same target
/// as before the move.  Absolute imports are left unchanged.
pub(crate) fn find_self_move_edits(
    text: &str,
    old_self_url: &Url,
    new_self_url: &Url,
) -> Vec<TextEdit> {
    use crate::util::import_graph::resolve_import;

    let mut edits = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();

        let (prefix, rest) = if let Some(r) = trimmed.strip_prefix("//import!") {
            ("//import!", r)
        } else if let Some(r) = trimmed.strip_prefix("//import") {
            if r.is_empty() || r.starts_with(' ') || r.starts_with('\t') {
                ("//import", r)
            } else {
                continue;
            }
        } else {
            if !trimmed.starts_with("//") {
                break;
            }
            continue;
        };

        let path_str = rest.trim();
        if path_str.is_empty() {
            continue;
        }

        // Absolute imports don't depend on the file's directory — skip.
        if is_absolute_import(path_str) {
            continue;
        }

        // Resolve against the OLD directory to find the actual target.
        let resolved = match resolve_import(old_self_url, path_str) {
            Some(r) => r,
            None => continue,
        };

        // Compute a new relative path from the NEW directory to the same target.
        let new_rel = match relative_path(new_self_url, &resolved.url) {
            Some(p) => p,
            None => continue,
        };

        // If the path didn't actually change, skip.
        if new_rel == path_str {
            continue;
        }

        let leading_ws = line.len() - trimmed.len();
        let prefix_end = leading_ws + prefix.len();
        let path_start_in_line = prefix_end + (rest.len() - rest.trim_start().len());
        let path_end_in_line = path_start_in_line + path_str.len();

        edits.push(TextEdit {
            range: Range {
                start: Position {
                    line: line_idx,
                    character: path_start_in_line,
                },
                end: Position {
                    line: line_idx,
                    character: path_end_in_line,
                },
            },
            new_text: new_rel,
        });
    }

    edits
}

/// Backward-compatible alias used by existing tests.
#[cfg(test)]
pub(crate) fn find_import_edits(
    text: &str,
    dep_uri: &Url,
    old_url: &Url,
    new_rel: &str,
) -> Vec<TextEdit> {
    // For old tests: derive a new_url by resolving new_rel against dep_uri.
    let new_url = match crate::util::import_graph::resolve_import(dep_uri, new_rel) {
        Some(r) => r.url,
        None => return vec![],
    };
    find_import_edits_for_target(text, dep_uri, old_url, &new_url)
}

#[cfg(test)]
#[path = "handle_test.rs"]
mod tests;



