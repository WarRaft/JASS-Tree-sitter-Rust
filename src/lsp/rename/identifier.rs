use crate::util::file_store::{is_uri_frozen, FILE_STORE};
use crate::lsp::position::Position;
use crate::lsp::rename::lsp::{PrepareRenameResult, TextEdit, WorkspaceEdit};
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::roper::uri_map::ROPE_MAP;
use std::collections::HashMap;
use url::Url;

// ─── Prepare rename ──────────────────────────────────────────────────────────

/// Check if the identifier at `position` can be renamed.
/// Returns `None` if nothing renamable is found, or the file is frozen.
pub fn prepare_rename(uri: &Url, position: &Position) -> Option<PrepareRenameResult> {
    if is_uri_frozen(uri) {
        return None;
    }

    let snapshot = FILE_STORE.get(uri)?;
    let ref_map = &snapshot.ref_map;
    let rope_entry = ROPE_MAP.get(uri)?;
    let byte_offset = position.to_byte_offset(rope_entry.value())?;

    let range = ref_map.range_at(byte_offset)?.clone();
    let name = ref_map.name_at(byte_offset)?.to_string();

    // Only rename symbols that exist in some FileSymbols.
    let _ = find_declaring_file(&name, uri)?;

    Some(PrepareRenameResult {
        range,
        placeholder: name,
    })
}

// ─── Rename ──────────────────────────────────────────────────────────────────

/// Compute a [`WorkspaceEdit`] that renames the identifier at `position` to
/// `new_name` across all connected files, skipping frozen files.
pub fn compute_identifier_rename(
    uri: &Url,
    position: &Position,
    new_name: &str,
) -> WorkspaceEdit {
    if is_uri_frozen(uri) {
        return WorkspaceEdit::default();
    }

    let (old_name, _byte) = match resolve_at(uri, position) {
        Some(t) => t,
        None => return WorkspaceEdit::default(),
    };

    let declaring_uri = match find_declaring_file(&old_name, uri) {
        Some(u) => u,
        None => return WorkspaceEdit::default(),
    };

    if is_uri_frozen(&declaring_uri) {
        return WorkspaceEdit::default();
    }

    // Collect all files that may reference this symbol.
    let mut files_to_scan = IMPORT_GRAPH.dependents(&declaring_uri);
    files_to_scan.push(declaring_uri.clone());
    files_to_scan.extend(IMPORT_GRAPH.dependencies(&declaring_uri));
    files_to_scan.sort();
    files_to_scan.dedup();

    let mut all_edits: HashMap<Url, Vec<TextEdit>> = HashMap::new();

    for file_uri in &files_to_scan {
        if is_uri_frozen(file_uri) {
            continue;
        }
        if file_uri != &declaring_uri && !can_see_symbol(file_uri, &declaring_uri) {
            continue;
        }

        // Use RefMap from FILE_STORE to find all occurrences.
        if let Some(snap) = FILE_STORE.get(file_uri) {
            let ref_map = &snap.ref_map;
            let edits: Vec<TextEdit> = ref_map
                .groups
                .values()
                .filter(|g| g.name == old_name)
                .flat_map(|g| &g.occurrences)
                .map(|occ| TextEdit {
                    range: occ.range.clone(),
                    new_text: new_name.to_string(),
                })
                .collect();
            if !edits.is_empty() {
                all_edits.insert(file_uri.clone(), edits);
            }
        }
    }

    WorkspaceEdit {
        changes: if all_edits.is_empty() {
            None
        } else {
            Some(all_edits)
        },
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Resolve the identifier at position → `(name, byte_offset)`.
fn resolve_at(uri: &Url, position: &Position) -> Option<(String, usize)> {
    let snapshot = FILE_STORE.get(uri)?;
    let ref_map = &snapshot.ref_map;
    let rope_entry = ROPE_MAP.get(uri)?;
    let byte_offset = position.to_byte_offset(rope_entry.value())?;
    let name = ref_map.name_at(byte_offset)?.to_string();
    Some((name, byte_offset))
}

fn can_see_symbol(viewer_uri: &Url, declaring_uri: &Url) -> bool {
    IMPORT_GRAPH.dependencies(viewer_uri).contains(declaring_uri)
}

fn find_declaring_file(name: &str, from_uri: &Url) -> Option<Url> {
    if let Some(snap) = FILE_STORE.get(from_uri) {
        if snap.file_symbols.has_symbol(name) {
            return Some(from_uri.clone());
        }
    }
    for dep_uri in &IMPORT_GRAPH.dependencies(from_uri) {
        if let Some(snap) = FILE_STORE.get(dep_uri) {
            if snap.file_symbols.has_symbol(name) {
                return Some(dep_uri.clone());
            }
        }
    }
    None
}
