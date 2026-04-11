//! Shared parse infrastructure used by every language.
//!
//! Extracts common patterns from `lng::jass::parse` and `lng::ass::parse`:
//!
//! * **Import resolution** — `resolve_import_directive` turns an
//!   `ImportDirective` into `(Url, DocumentLink, Diagnostic)`.
//! * **`GlobalEntry` / `SymbolNS`** — cross-file symbol types.
//! * **`file_symbols_to_entries`** — converts `FileSymbols` into
//!   `GlobalEntry` items for cross-file resolution.
//! * **`all_visible_entries`** — collects entries from a set of URIs
//!   via `peek_or_load` + `file_symbols_to_entries`.
//! * **`compute_hash_for_uri`** — content hash from ROPE_MAP or disk.
//! * **`ensure_file_symbols`** — loads symbols for a dependency URI
//!   from cache or parses from disk, parameterized by `tree_sitter::Language`.
//! * **`cascade_parse_and_notify`** — the two-pass cascade loop
//!   (CascadeGuard + pending-drain + peer re-parse) with pluggable
//!   per-language parse functions.

use crate::lng::directive::ImportDirective;
use crate::lng::directive::UjapiDirective;
use crate::lng::symbol::FileSymbols;
use crate::http::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::http::document_link::DocumentLink;
use crate::http::position::Position;
use crate::http::range::Range;
use crate::http::ref_map::{DeclKey, RefMap};
use crate::util::file_cache;
use crate::util::parse_cache::{
    drain_pending, peek_or_load, mark_parse_done,
    CascadeGuard, PARSE_CACHE,
    MAX_CASCADE_PEERS, REPARSE_GUARD,
};
use crate::util::import_graph::{resolve_import, IMPORT_GRAPH};
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::tree_map::TREE_MAP;
use lapce_xi_rope::Rope;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use url::Url;

// ─── Symbol types (moved from scope_resolver) ───────────────────────────────

/// Symbol namespace — JASS separates functions and variables/types by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolNS {
    /// `function` or `native` — resolves in the function namespace.
    Func,
    /// Global variable, constant, or `type` — resolves in the variable namespace.
    Var,
}

/// A single global-scope declaration in one file.
///
/// Contains enough information for cross-file resolution, hover tooltips,
/// and completion items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalEntry {
    /// URI of the file containing the declaration.
    pub uri: Url,
    /// Symbol name.
    pub name: String,
    /// Namespace — JASS always uses `""` (global scope).
    /// AngelScript will use the enclosing `namespace Foo { … }` name.
    pub namespace: String,
    /// Namespace.
    pub ns: SymbolNS,
    /// `start_byte` of the declaring node in the origin file.
    /// Used as `DeclKey` for cross-file reference linking.
    pub decl_key: usize,

    // ── Metadata for hover / completion ──────────────────────────────────

    /// Type name for variables; `None` for functions/natives.
    pub type_name: Option<String>,
    /// Parameter list `(name, type)` for functions/natives; empty for vars.
    pub params: Vec<(String, String)>,
    /// Return type for functions/natives; `None` for variables.
    pub return_type: Option<String>,
    /// `true` for `constant` global variables.
    pub is_constant: bool,
    /// `true` for `array` global variables.
    pub is_array: bool,
    /// `//*` doc comment (markdown) attached to this declaration.
    pub doc_comment: Option<String>,
}

// ─── Helpers that replace SCOPE_RESOLVER ─────────────────────────────────────

/// Return all global entries from files in `visible_uris`.
///
/// Iterates each URI, loads the snapshot via [`peek_or_load`] (read-only,
/// does NOT insert into `PARSE_CACHE`), and converts `FileSymbols` to
/// `GlobalEntry` items.  This replaces `SCOPE_RESOLVER.all_visible()`.
pub fn all_visible_entries(visible_uris: &HashSet<Url>) -> Vec<GlobalEntry> {
    let mut result = Vec::new();
    for uri in visible_uris {
        if let Some(snap) = peek_or_load(uri) {
            result.extend(file_symbols_to_entries(uri, &snap.file_symbols));
        }
    }
    result
}

/// Look up all entries for `name` in namespace `ns`, filtered to
/// declarations from `visible_uris`.
///
/// Replaces `SCOPE_RESOLVER.resolve()`.
pub fn resolve_entries(name: &str, ns: SymbolNS, visible: &HashSet<Url>) -> Vec<GlobalEntry> {
    let mut result = Vec::new();
    for uri in visible {
        if let Some(snap) = peek_or_load(uri) {
            for entry in file_symbols_to_entries(uri, &snap.file_symbols) {
                if entry.name == name && entry.ns == ns {
                    result.push(entry);
                }
            }
        }
    }
    result
}

/// Return all entries for a single `uri`.
///
/// Replaces `SCOPE_RESOLVER.entries_for_uri()`.
pub fn entries_for_uri(uri: &Url) -> Vec<GlobalEntry> {
    match peek_or_load(uri) {
        Some(snap) => file_symbols_to_entries(uri, &snap.file_symbols),
        None => Vec::new(),
    }
}

/// Return all entries across every file known to `IMPORT_GRAPH` and `PARSE_CACHE`.
///
/// Replaces `SCOPE_RESOLVER.all_entries()`.
pub fn all_entries() -> Vec<GlobalEntry> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    // Collect from IMPORT_GRAPH (covers all known files).
    for uri in IMPORT_GRAPH.all_uris() {
        if seen.insert(uri.clone()) {
            if let Some(snap) = peek_or_load(&uri) {
                result.extend(file_symbols_to_entries(&uri, &snap.file_symbols));
            }
        }
    }

    // Also collect from PARSE_CACHE (covers open files not yet in the graph).
    for entry in PARSE_CACHE.iter() {
        let uri = entry.key().clone();
        if seen.insert(uri.clone()) {
            result.extend(file_symbols_to_entries(&uri, &entry.value().file_symbols));
        }
    }

    result
}

// ─── Import resolution ──────────────────────────────────────────────────────

/// Resolve a single `//import` / `//import!` directive.
///
/// On success pushes into `imports`, `frozen_imports`, `links`;
/// on failure pushes into `diagnostics`.
pub fn resolve_import_directive(
    uri: &Url,
    imp: &ImportDirective,
    src: &[u8],
    rope: &Rope,
    imports: &mut HashSet<Url>,
    frozen_imports: &mut HashSet<Url>,
    links: &mut Vec<DocumentLink>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if imp.path.is_empty() {
        return; // cursor already emits "Missing import path"
    }

    let node = &imp.node;
    let prefix_len = if imp.frozen {
        "//import!".len()
    } else {
        "//import".len()
    };

    let node_text =
        std::str::from_utf8(&src[node.start_byte()..node.end_byte()]).unwrap_or("");
    let after_prefix = &node_text[prefix_len..];
    let ws_len = after_prefix.len() - after_prefix.trim_start().len();
    let path_start_byte = node.start_byte() + prefix_len + ws_len;
    let path_end_byte = node.start_byte() + prefix_len + ws_len + imp.path.len();

    let path_range = Range {
        start: Position::from_byte_offset(rope, path_start_byte).unwrap_or_default(),
        end: Position::from_byte_offset(rope, path_end_byte).unwrap_or_default(),
    };

    match resolve_import(uri, &imp.path) {
        Some(resolved) => {
            imports.insert(resolved.url.clone());
            if imp.frozen {
                frozen_imports.insert(resolved.url.clone());
            }
            if resolved.exists {
                links.push(DocumentLink {
                    range: path_range.clone(),
                    target: Some(resolved.url.to_string()),
                    tooltip: Some(resolved.url.to_string()),
                });
                // Emit a hint when the target file exists but hasn't been
                // opened/parsed yet — the quick fix will open it.
                if !PARSE_CACHE.contains_key(&resolved.url) {
                    diagnostics.push(Diagnostic {
                        range: path_range,
                        message: crate::util::i18n::import_file_not_opened(&imp.path),
                        severity: Some(DiagnosticSeverity::Hint),
                        data: Some(serde_json::json!({
                            "open_uri": resolved.url.to_string(),
                        })),
                        ..Diagnostic::new("jass", "import-not-opened")
                    });
                }
            } else {
                diagnostics.push(Diagnostic {
                    range: path_range,
                    message: crate::util::i18n::file_not_found(&imp.path),
                    severity: Some(DiagnosticSeverity::Error),
                    ..Diagnostic::new("jass", "import-not-found")
                });
            }
        }
        None => {
            diagnostics.push(Diagnostic {
                range: path_range,
                message: crate::util::i18n::cannot_resolve_import(&imp.path),
                severity: Some(DiagnosticSeverity::Error),
                ..Diagnostic::new("jass", "import-resolve")
            });
        }
    }
}

/// Resolve a single `//import-ujapi! <path>` directive.
///
/// The target file is treated as a **frozen** import.
///
/// ## Logic
///
/// 1. Resolve `<path>` to an absolute path.
/// 2. Read the **first line** of the file to extract `//<tag>`.
/// 3. Compare the local tag with the cached latest GitHub release tag.
///
/// | File exists? | Tag matches latest? | Result |
/// |:---:|:---:|---|
/// | ✗ | — | Error diagnostic + code action "download" |
/// | ✓ | no tag in file | Warning diagnostic + code action "re-download" |
/// | ✓ | tag ≠ latest | Warning diagnostic + code action "re-download" |
/// | ✓ | tag = latest (or latest unknown) | document link + inlay hint |
///
/// Version info is shown as an **InlayHint** after the path, not as a
/// Hint-level diagnostic.
pub fn resolve_ujapi_directive(
    uri: &Url,
    ud: &UjapiDirective,
    src: &[u8],
    rope: &Rope,
    imports: &mut HashSet<Url>,
    frozen_imports: &mut HashSet<Url>,
    links: &mut Vec<DocumentLink>,
    diagnostics: &mut Vec<Diagnostic>,
    inlay_hints: &mut Vec<crate::http::inlay_hint::InlayHint>,
) {
    use crate::util::ujapi;

    if ud.path.is_empty() {
        return; // cursor already emits "Missing destination path"
    }

    let node = &ud.node;
    let prefix_len = "//import-ujapi!".len();

    let node_text =
        std::str::from_utf8(&src[node.start_byte()..node.end_byte()]).unwrap_or("");
    let after_prefix = &node_text[prefix_len..];
    let ws_len = after_prefix.len() - after_prefix.trim_start().len();
    let path_start_byte = node.start_byte() + prefix_len + ws_len;
    let path_end_byte = node.start_byte() + prefix_len + ws_len + ud.path.len();

    let path_range = Range {
        start: Position::from_byte_offset(rope, path_start_byte).unwrap_or_default(),
        end: Position::from_byte_offset(rope, path_end_byte).unwrap_or_default(),
    };

    // Lazily schedule a background version check (once per session).
    ujapi::schedule_background_check();

    match resolve_import(uri, &ud.path) {
        Some(resolved) => {
            // Always register as a frozen import (even when outdated).
            imports.insert(resolved.url.clone());
            frozen_imports.insert(resolved.url.clone());

            // Data payload for code actions (download / re-download).
            let ujapi_data = serde_json::json!({
                "ujapi_uri": uri.to_string(),
                "ujapi_path": ud.path,
            });

            if !resolved.exists {
                // ── File does not exist ──────────────────────────────
                diagnostics.push(Diagnostic {
                    range: path_range,
                    message: crate::util::i18n::ujapi_file_not_found(&ud.path),
                    severity: Some(DiagnosticSeverity::Error),
                    data: Some(ujapi_data),
                    ..Diagnostic::new("jass", "ujapi")
                });
                return;
            }

            // ── File exists — check version tag ─────────────────────
            let disk_path = resolved.url.to_file_path().ok();
            let file_tag = disk_path
                .as_deref()
                .and_then(ujapi::read_file_tag);

            let latest = ujapi::cached_release();

            match (&file_tag, &latest) {
                // No tag in the file at all → broken
                (None, _) => {
                    diagnostics.push(Diagnostic {
                        range: path_range.clone(),
                        message: crate::util::i18n::ujapi_no_version_tag().into(),
                        severity: Some(DiagnosticSeverity::Warning),
                        data: Some(ujapi_data.clone()),
                        ..Diagnostic::new("jass", "ujapi")
                    });
                }
                // We have both tags and they differ → outdated
                (Some(ft), Some(rel)) if *ft != rel.tag => {
                    diagnostics.push(Diagnostic {
                        range: path_range.clone(),
                        message: crate::util::i18n::ujapi_outdated(ft, &rel.tag),
                        severity: Some(DiagnosticSeverity::Warning),
                        data: Some(ujapi_data.clone()),
                        ..Diagnostic::new("jass", "ujapi")
                    });
                }
                // Tags match → show version as inlay hint ✓
                (Some(ft), Some(rel)) if *ft == rel.tag => {
                    inlay_hints.push(crate::http::inlay_hint::InlayHint {
                        position: path_range.end.clone(),
                        label: format!("{} ✓", ft),
                        kind: crate::http::inlay_hint::InlayHintKind::None,
                        byte_offset: path_end_byte,
                    });
                }
                // File has tag but no cached release — show version as inlay hint
                (Some(ft), None) => {
                    inlay_hints.push(crate::http::inlay_hint::InlayHint {
                        position: path_range.end.clone(),
                        label: format!("{}", ft),
                        kind: crate::http::inlay_hint::InlayHintKind::None,
                        byte_offset: path_end_byte,
                    });
                }
                _ => {}
            }

            // Build tooltip.
            let tooltip = match (&file_tag, &latest) {
                (Some(ft), Some(rel)) if *ft == rel.tag => {
                    crate::util::i18n::ujapi_tooltip_up_to_date(ft)
                }
                (Some(ft), Some(rel)) => {
                    crate::util::i18n::ujapi_tooltip_update_available(ft, &rel.tag)
                }
                (Some(ft), None) => {
                    format!("UjAPI {}", ft)
                }
                (None, _) => crate::util::i18n::ujapi_tooltip_no_tag().into(),
            };

            links.push(DocumentLink {
                range: path_range,
                target: Some(resolved.url.to_string()),
                tooltip: Some(tooltip),
            });
        }
        None => {
            diagnostics.push(Diagnostic {
                range: path_range,
                message: crate::util::i18n::ujapi_cannot_resolve(&ud.path),
                severity: Some(DiagnosticSeverity::Error),
                ..Diagnostic::new("jass", "ujapi-resolve")
            });
        }
    }
}

/// Resolve a path (e.g. from an AS `#include` directive) given its already-
/// computed range.
///
/// Pushes into `imports`, `links`, or `diagnostics`.
pub fn resolve_path_import(
    uri: &Url,
    path_text: &str,
    path_range: Range,
    imports: &mut HashSet<Url>,
    links: &mut Vec<DocumentLink>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match resolve_import(uri, path_text) {
        Some(resolved) => {
            imports.insert(resolved.url.clone());
            if resolved.exists {
                links.push(DocumentLink {
                    range: path_range.clone(),
                    target: Some(resolved.url.to_string()),
                    tooltip: Some(resolved.url.to_string()),
                });
                if !PARSE_CACHE.contains_key(&resolved.url) {
                    diagnostics.push(Diagnostic {
                        range: path_range,
                        message: crate::util::i18n::import_file_not_opened(path_text),
                        severity: Some(DiagnosticSeverity::Hint),
                        data: Some(serde_json::json!({
                            "open_uri": resolved.url.to_string(),
                        })),
                        ..Diagnostic::new("jass", "import-not-opened")
                    });
                }
            } else {
                diagnostics.push(Diagnostic {
                    range: path_range,
                    message: crate::util::i18n::file_not_found(path_text),
                    severity: Some(DiagnosticSeverity::Error),
                    ..Diagnostic::new("jass", "import-not-found")
                });
            }
        }
        None => {
            diagnostics.push(Diagnostic {
                range: path_range,
                message: crate::util::i18n::cannot_resolve_import(path_text),
                severity: Some(DiagnosticSeverity::Error),
                ..Diagnostic::new("jass", "import-resolve")
            });
        }
    }
}

// ─── Symbol helpers ─────────────────────────────────────────────────────────

/// Convert unified `FileSymbols` into `GlobalEntry` items for the scope resolver.
///
/// Works for both JASS and AS — iterates all collections; unused ones
/// are simply empty.
pub fn file_symbols_to_entries(uri: &Url, fs: &FileSymbols) -> Vec<GlobalEntry> {
    let mut entries = Vec::new();

    for f in &fs.functions {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: f.name.clone(),
            namespace: f.namespace.clone(),
            ns: SymbolNS::Func,
            decl_key: f.decl_byte,
            type_name: None,
            params: f.params.iter().map(|p| (p.name.clone(), p.type_name.clone())).collect(),
            return_type: f.return_type.clone(),
            is_constant: f.is_constant,
            is_array: false,
            doc_comment: f.doc_comment.clone(),
        });
    }
    for n in &fs.natives {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: n.name.clone(),
            namespace: String::new(),
            ns: SymbolNS::Func,
            decl_key: 0,
            type_name: None,
            params: n.params.iter().map(|p| (p.name.clone(), p.type_name.clone())).collect(),
            return_type: n.return_type.clone(),
            is_constant: n.is_constant,
            is_array: false,
            doc_comment: n.doc_comment.clone(),
        });
    }
    for g in &fs.globals {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: g.name.clone(),
            namespace: g.namespace.clone(),
            ns: SymbolNS::Var,
            decl_key: g.decl_byte,
            type_name: g.type_name.clone(),
            params: vec![],
            return_type: None,
            is_constant: g.is_constant,
            is_array: g.is_array,
            doc_comment: g.doc_comment.clone(),
        });
    }
    for t in &fs.types {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: t.name.clone(),
            namespace: String::new(),
            ns: SymbolNS::Var,
            decl_key: 0,
            type_name: t.base.clone(),
            params: vec![],
            return_type: None,
            is_constant: false,
            is_array: false,
            doc_comment: t.doc_comment.clone(),
        });
    }
    // ── AS-specific ──────────────────────────────────────────────────
    for c in &fs.classes {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: c.name.clone(),
            namespace: c.namespace.clone(),
            ns: SymbolNS::Var,
            decl_key: c.decl_byte,
            type_name: None,
            params: vec![],
            return_type: None,
            is_constant: false,
            is_array: false,
            doc_comment: c.doc_comment.clone(),
        });
    }
    for i in &fs.interfaces {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: i.name.clone(),
            namespace: i.namespace.clone(),
            ns: SymbolNS::Var,
            decl_key: i.decl_byte,
            type_name: None,
            params: vec![],
            return_type: None,
            is_constant: false,
            is_array: false,
            doc_comment: i.doc_comment.clone(),
        });
    }
    for e in &fs.enums {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: e.name.clone(),
            namespace: e.namespace.clone(),
            ns: SymbolNS::Var,
            decl_key: e.decl_byte,
            type_name: None,
            params: vec![],
            return_type: None,
            is_constant: false,
            is_array: false,
            doc_comment: e.doc_comment.clone(),
        });
    }
    for m in &fs.mixins {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: m.name.clone(),
            namespace: m.namespace.clone(),
            ns: SymbolNS::Var,
            decl_key: m.decl_byte,
            type_name: None,
            params: vec![],
            return_type: None,
            is_constant: false,
            is_array: false,
            doc_comment: m.doc_comment.clone(),
        });
    }
    for td in &fs.typedefs {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: td.alias.clone(),
            namespace: td.namespace.clone(),
            ns: SymbolNS::Var,
            decl_key: td.decl_byte,
            type_name: Some(td.original.clone()),
            params: vec![],
            return_type: None,
            is_constant: false,
            is_array: false,
            doc_comment: td.doc_comment.clone(),
        });
    }
    for fd in &fs.funcdefs {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: fd.name.clone(),
            namespace: fd.namespace.clone(),
            ns: SymbolNS::Func,
            decl_key: fd.decl_byte,
            type_name: None,
            params: fd.params.iter().map(|p| (p.name.clone(), p.type_name.clone())).collect(),
            return_type: fd.return_type.clone(),
            is_constant: false,
            is_array: false,
            doc_comment: fd.doc_comment.clone(),
        });
    }

    entries
}

// ─── Recursive import extraction ────────────────────────────────────────────

/// Recursively collect all import URLs from an AS AST.
///
/// Walks `TopLevel::Namespace` children so that `#include` directives
/// nested inside namespace blocks are discovered (the namespace scope
/// does not affect the import — all includes go to the global scope).
pub fn collect_as_imports(
    items: &[crate::lng::ass::ast::TopLevel],
    uri: &Url,
    rope: &Rope,
) -> HashSet<Url> {
    use crate::lng::ass::ast::TopLevel;
    use crate::util::roper::node::NodeExt;

    let mut imports = HashSet::new();
    for item in items {
        match item {
            TopLevel::Include(incl) => {
                if let Some(path_node) = &incl.path {
                    let path_text = path_node.text(rope);
                    if let Some(resolved) = resolve_import(uri, &path_text) {
                        imports.insert(resolved.url);
                    }
                }
            }
            TopLevel::ImportDir(imp) => {
                if !imp.path.is_empty() {
                    if let Some(resolved) = resolve_import(uri, &imp.path) {
                        imports.insert(resolved.url);
                    }
                }
            }
            TopLevel::UjapiDir(ud) => {
                if !ud.path.is_empty() {
                    if let Some(resolved) = resolve_import(uri, &ud.path) {
                        imports.insert(resolved.url);
                    }
                }
            }
            TopLevel::Namespace(ns) => {
                // Recurse into namespace body — #include inside namespace
                // still imports to the global scope.
                imports.extend(collect_as_imports(&ns.body, uri, rope));
            }
            _ => {}
        }
    }
    imports
}

/// Collect frozen-import URLs from an AS AST.
///
/// Walks `TopLevel::ImportDir` (frozen) and `TopLevel::UjapiDir`
/// recursively — namespace blocks are entered so that directives nested
/// inside them are discovered.
fn collect_as_frozen_imports(
    items: &[crate::lng::ass::ast::TopLevel],
    uri: &Url,
    src: &[u8],
    rope: &Rope,
) -> HashSet<Url> {
    use crate::lng::ass::ast::TopLevel;

    let mut frozen = HashSet::new();
    for item in items {
        if let TopLevel::ImportDir(imp) = item {
            if imp.frozen && !imp.path.is_empty() {
                if let Some(resolved) = resolve_import(uri, &imp.path) {
                    frozen.insert(resolved.url);
                }
            }
        }
        if let TopLevel::UjapiDir(ud) = item {
            // ujapi directives are always frozen
            if !ud.path.is_empty() {
                if let Some(resolved) = resolve_import(uri, &ud.path) {
                    frozen.insert(resolved.url);
                }
            }
        }
        if let TopLevel::Namespace(ns) = item {
            frozen.extend(collect_as_frozen_imports(&ns.body, uri, src, rope));
        }
    }
    frozen
}


/// Ensure that `file_cache` has entries for `dep_uri`.
///
/// **Must NOT be called for the file currently being parsed** — the caller
/// must exclude `current_uri` from the loop to avoid clobbering in-flight data.
///
/// The tree-sitter language is selected automatically based on the URI
/// extension (`.as` → AngelScript, otherwise → JASS).
///
/// Returns `true` if symbols were successfully loaded, `false` otherwise.
///
/// ## Staleness checks
///
/// 1. **In-memory** (`PARSE_CACHE`) — cheapest, no I/O.
/// 2. **Disk cache** (`file_cache`) — one `stat()` call to validate
///    `FileMeta` (size + mtime).  If fresh, returns immediately.
///    Handlers that need data later call [`peek_or_load`].
/// 3. **Parse from disk** — last resort, reads + parses the file.
pub fn ensure_file_symbols(dep_uri: &Url, ts_language: tree_sitter::Language) -> bool {
    // Already fully parsed — PARSE_CACHE has everything.
    if PARSE_CACHE.contains_key(dep_uri) {
        return true;
    }

    // Disk cache — if file metadata matches, populate SCOPE_RESOLVER only.
    // The full snapshot stays on disk and will be lazy-loaded by
    // `peek_or_load()` if any handler actually needs it.
    if let Some(cached) = file_cache::load_if_fresh(dep_uri) {
        // Persist entry-point status so recompute_entry_cache sees it.
        if cached.symbols.is_entry {
            IMPORT_GRAPH.mark_entry(dep_uri, true);
        }
        return true;
    }

    // Parse from disk.
    let path = match dep_uri.to_file_path() {
        Ok(p) if p.is_file() => p,
        _ => return false,
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&ts_language).is_err() {
        return false;
    }
    let tree = match parser.parse(&content, None) {
        Some(t) => t,
        None => return false,
    };

    let rope = Rope::from(content.as_str());
    let hash = file_cache::content_hash(&rope);

    // Dispatch AST building by file extension so AS files use the correct
    // grammar/cursor and vice-versa.
    let is_as = crate::util::open::is_as_uri(dep_uri);

    if is_as {
        // ── AngelScript path ──
        let mut ast = crate::lng::ass::ast::build_ast(tree.root_node());
        let src: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
        crate::lng::ass::ast::rewrite_directives(&mut ast, &src);

        // Extract imports (recursively — handles #include inside namespace
        // blocks) and register them in the import graph so that transitive
        // dependencies are visible to `visible_component`.
        let dep_imports = collect_as_imports(&ast.items, dep_uri, &rope);
        if !dep_imports.is_empty() {
            IMPORT_GRAPH.update(dep_uri, dep_imports);
        }

        // Also extract frozen import URLs for `is_uri_frozen()`.
        let frozen_imports = collect_as_frozen_imports(&ast.items, dep_uri, &src, &rope);
        IMPORT_GRAPH.mark_frozen(&frozen_imports);

        let cursor = crate::lng::ass::cursor::Cursor::walk(&ast, &rope, &[]);

        let mut as_file_symbols = cursor.file_symbols;
        as_file_symbols.file_settings = cursor.file_settings;
        as_file_symbols.frozen_imports = frozen_imports;

        // Build a RefMap for cross-file go-to-definition.
        let func_decl_keys = cursor.func_decl_keys;
        let var_decl_keys = cursor.var_decl_keys;
        let arg_decl_keys = cursor.arg_decl_keys;
        let ref_map = crate::http::ref_map::build_ref_map(
            cursor.ref_groups,
            cursor.ref_names,
            cursor.external_decls,
            &rope,
        );

        // Convert AS symbols to unified FileSymbols
        let file_symbols = as_file_symbols.to_unified();

        // Persist to disk cache — no in-memory snapshot for closed files.
        if let Some(meta) = file_cache::FileMeta::from_uri(dep_uri) {
            file_cache::store(
                dep_uri,
                meta,
                hash,
                &file_symbols,
                &ref_map,
                &func_decl_keys,
                &var_decl_keys,
                &arg_decl_keys,
            );
        }

        // Closed file — snapshot stays in redb, not in memory.
        // Handlers use `peek_or_load()` to lazy-load if needed.
        // Persist entry-point status so recompute_entry_cache sees it.
        if file_symbols.is_entry {
            IMPORT_GRAPH.mark_entry(dep_uri, true);
        }

        return true;
    }

    // ── JASS path (default) ──
    let mut ast = crate::lng::jass::ast::build_ast(tree.root_node());
    let src: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
    crate::lng::jass::ast::rewrite_imports(&mut ast, &src);

    // Extract imports and register them in the import graph so that
    // transitive dependencies are visible to `visible_component`.
    // Also collect frozen import URLs for `is_uri_frozen()`.
    let mut dep_imports = HashSet::new();
    let mut frozen_imports = HashSet::new();
    for item in &ast.items {
        if let crate::lng::jass::ast::Statement::Import(imp) = item {
            if !imp.path.is_empty() {
                if let Some(resolved) = resolve_import(dep_uri, &imp.path) {
                    dep_imports.insert(resolved.url.clone());
                    if imp.frozen {
                        frozen_imports.insert(resolved.url);
                    }
                }
            }
        }
        if let crate::lng::jass::ast::Statement::UjapiImport(ud) = item {
            if !ud.path.is_empty() {
                if let Some(resolved) = resolve_import(dep_uri, &ud.path) {
                    dep_imports.insert(resolved.url.clone());
                    // ujapi imports are always frozen
                    frozen_imports.insert(resolved.url);
                }
            }
        }
    }
    if !dep_imports.is_empty() {
        IMPORT_GRAPH.update(dep_uri, dep_imports);
    }
    IMPORT_GRAPH.mark_frozen(&frozen_imports);

    let cursor = crate::lng::jass::cursor::Cursor::walk(&ast, &rope, &[]);
    let mut file_symbols = cursor.file_symbols;
    file_symbols.frozen_imports = frozen_imports;
    let hash = file_cache::content_hash(&rope);

    // Build a full RefMap so that go-to-definition can find declaration
    // positions and `find_decl_key_by_name` can resolve the real DeclKey.
    let func_decl_keys = cursor.func_decl_keys;
    let var_decl_keys = cursor.var_decl_keys;
    let arg_decl_keys = cursor.arg_decl_keys;
    let ref_map = crate::http::ref_map::build_ref_map(
        cursor.ref_groups,
        cursor.ref_names,
        cursor.external_decls,
        &rope,
    );

    // Persist to disk cache — no in-memory snapshot for closed files.
    if let Some(meta) = file_cache::FileMeta::from_uri(dep_uri) {
        file_cache::store(
            dep_uri,
            meta,
            hash,
            &file_symbols,
            &ref_map,
            &func_decl_keys,
            &var_decl_keys,
            &arg_decl_keys,
        );
    }

    // Closed file — snapshot stays in redb, not in memory.
    // Handlers use `peek_or_load()` to lazy-load if needed.
    // Persist entry-point status so recompute_entry_cache sees it.
    if file_symbols.is_entry {
        IMPORT_GRAPH.mark_entry(dep_uri, true);
    }
    true
}

/// Look up the `DeclKey` of a symbol by name **and namespace** in a `RefMap`.
pub fn find_decl_key_by_name(
    ref_map: &RefMap,
    name: &str,
    ns: SymbolNS,
    func_keys: &HashSet<DeclKey>,
) -> Option<DeclKey> {
    for (&key, group) in &ref_map.groups {
        if group.name != name || !group.occurrences.iter().any(|o| o.is_decl) {
            continue;
        }
        let is_func = func_keys.contains(&key);
        match ns {
            SymbolNS::Func if is_func => return Some(key),
            SymbolNS::Var if !is_func => return Some(key),
            _ => continue,
        }
    }
    None
}

// ─── Cascade ────────────────────────────────────────────────────────────────

/// Type alias for async parse functions passed to the cascade helper.
pub type ParseFn = Box<
    dyn Fn(Url) -> Pin<Box<dyn Future<Output = Result<Vec<Url>, Box<dyn Error + Send + Sync>>> + Send>>
        + Send
        + Sync,
>;

// ─── Universal dispatch ─────────────────────────────────────────────────────

/// Parse an in-memory file (must have rope + tree), dispatching to the
/// correct language based on the URI extension.
pub async fn parse_by_uri(uri: &Url) -> Result<Vec<Url>, Box<dyn Error + Send + Sync>> {
    if crate::util::open::is_as_uri(uri) {
        crate::lng::ass::parse::parse(uri).await
    } else {
        crate::lng::jass::parse::parse(uri).await
    }
}

/// Parse a closed file from disk, dispatching to the correct language
/// based on the URI extension.
pub async fn parse_from_disk_by_uri(uri: &Url) -> Result<Vec<Url>, Box<dyn Error + Send + Sync>> {
    if crate::util::open::is_as_uri(uri) {
        crate::lng::ass::parse::parse_from_disk(uri).await
    } else {
        crate::lng::jass::parse::parse_from_disk(uri).await
    }
}

/// Two-pass cascade: parse the current file, drain pending waiters,
/// re-parse affected peers, refresh editors.
///
/// Diagnostics use the **push** model: after all parsing is done,
/// `send_refresh_all` sends `textDocument/publishDiagnostics` for every
/// file in `PARSE_CACHE` — both open and closed tabs see errors immediately.
///
/// # Arguments
///
/// * `uri` — the file that was just edited.
/// * `parse_fn` — language-specific parse that returns the cascade peer list.
/// * `parse_from_disk_fn` — language-specific disk parse for closed peers.
///
/// **Peer re-parsing** dispatches by URI extension (via [`parse_by_uri`] /
/// [`parse_from_disk_by_uri`]) so that cross-language imports (JASS ↔ AS)
/// always use the correct grammar.
pub async fn cascade_parse_and_notify(
    uri: &Url,
    parse_fn: &ParseFn,
    _parse_from_disk_fn: Option<&ParseFn>,
    generation: Option<u64>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Cycle guard.
    let _guard = match CascadeGuard::try_enter(uri) {
        Some(g) => g,
        None => {
            log::debug!("cascade: skipping {} (already in progress)", uri.path());
            return Ok(());
        }
    };

    // Pass 1: parse the current file.
    let mut cascade = parse_fn(uri.clone()).await?;

    // Drain pending-import waiters.
    let pending_waiters = drain_pending(uri);
    for waiter in pending_waiters {
        if !cascade.contains(&waiter) {
            cascade.push(waiter);
        }
    }

    // Pass 2: cascade re-parse affected peers.
    let mut count = 0usize;

    for peer_uri in &cascade {
        if count >= MAX_CASCADE_PEERS {
            log::warn!(
                "cascade: capped at {} peers for {}",
                MAX_CASCADE_PEERS,
                uri.path()
            );
            break;
        }

        if REPARSE_GUARD.contains(peer_uri) {
            log::debug!("cascade: skipping {} (guarded)", peer_uri.path());
            continue;
        }

        let ok = if ROPE_MAP.contains_key(peer_uri) && TREE_MAP.contains_key(peer_uri) {
            match parse_by_uri(peer_uri).await {
                Ok(_) => true,
                Err(e) => {
                    log::error!("cascade re-parse {}: {}", peer_uri, e);
                    false
                }
            }
        } else {
            match parse_from_disk_by_uri(peer_uri).await {
                Ok(_) => true,
                Err(e) => {
                    log::debug!("cascade disk-parse {}: {}", peer_uri.path(), e);
                    false
                }
            }
        };

        if ok {
            count += 1;
        }
    }


    // Signal that this parse generation is complete.
    if let Some(g) = generation {
        mark_parse_done(uri, g);
    }

    Ok(())
}

