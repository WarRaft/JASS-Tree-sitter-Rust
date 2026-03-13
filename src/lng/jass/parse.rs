use crate::lng::jass::ast::{build_ast, rewrite_imports, Statement};
use crate::lng::jass::cursor::{Cursor, ImportedKind, ImportedSymbol};
use crate::lng::jass::symbol::FILE_SYMBOLS;
use crate::lng::jass::uri_map::TREE_MAP;
use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity, DocumentDiagnosticReport};
use crate::lsp::diagnostic::uri_map::URI_MAP as DIAGNOSTIC_URI_MAP;
use crate::lsp::document_link::lsp::DocumentLink;
use crate::lsp::document_link::uri_map::URI_MAP as LINK_URI_MAP;
use crate::lsp::document_symbol::uri_map::URI_MAP as SYMBOL_URI_MAP;
use crate::lsp::folding::uri_map::URI_MAP as FOLDING_URI_MAP;
use crate::lsp::position::Position;
use crate::lsp::ref_map::{build_ref_map, RefMap, REF_URI_MAP};
use crate::lsp::range::Range;
use crate::lsp::semantic::uri_map::URI_MAP as SEMANTIC_URI_MAP;
use crate::util::import_graph::{resolve_import, IMPORT_GRAPH};
use crate::util::ref_cache;
use crate::util::scope_resolver::{GlobalEntry, SymbolNS, SCOPE_RESOLVER};
use crate::util::symbol_cache;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_lock::uri_unlock;
use lapce_xi_rope::Rope;
use std::collections::HashSet;
use std::error::Error;
use url::Url;

// ─── Main parse entry point ─────────────────────────────────────────────────

/// Look up the DeclKey of a symbol by name in a RefMap.
fn find_decl_key_by_name(ref_map: &RefMap, name: &str) -> Option<usize> {
    for (&key, group) in &ref_map.groups {
        if group.name == name && group.occurrences.iter().any(|o| o.is_decl) {
            return Some(key);
        }
    }
    None
}

/// Ensure that `FILE_SYMBOLS` and `SCOPE_RESOLVER` have entries for `dep_uri`.
///
/// Resolution order:
/// 1. Already in memory (`SCOPE_RESOLVER` knows the URI) → done.
/// 2. Disk cache (`symbol_cache`) → load into memory + populate resolver.
/// 3. Parse the file from disk (lightweight — no imported symbols, just local
///    declarations).  Result is cached for future use.
fn ensure_file_symbols(dep_uri: &Url) {
    // 1. Already known to the resolver → skip (FILE_SYMBOLS populated below).
    if !SCOPE_RESOLVER.is_stale(dep_uri, &[0u8; 32]) && FILE_SYMBOLS.contains_key(dep_uri) {
        return;
    }

    // 2. Disk cache
    if let Some((_meta, symbols)) = symbol_cache::load(dep_uri) {
        let entries = file_symbols_to_entries(dep_uri, &symbols);
        // Use a zero hash here — the real hash will be set when the file is
        // fully parsed (or from symbol_cache metadata).
        let rope_hash = compute_hash_for_uri(dep_uri);
        SCOPE_RESOLVER.update_file(dep_uri, rope_hash, entries);
        FILE_SYMBOLS.insert(dep_uri.clone(), symbols);
        return;
    }

    // 3. Parse from disk
    let path = match dep_uri.to_file_path() {
        Ok(p) if p.exists() => p,
        _ => return,
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_jass::language().into())
        .is_err()
    {
        return;
    }
    let tree = match parser.parse(&content, None) {
        Some(t) => t,
        None => return,
    };
    let rope = Rope::from(content.as_str());
    let ast = build_ast(tree.root_node());
    // Lightweight walk — no imports, just collect local declarations.
    let cursor = Cursor::walk(&ast, &rope, &[]);
    let file_symbols = cursor.file_symbols;
    // Persist to disk cache for next startup.
    if let Some(meta) = symbol_cache::FileMeta::from_uri(dep_uri) {
        symbol_cache::store(dep_uri, meta, &file_symbols);
    }
    // Populate scope resolver
    let entries = file_symbols_to_entries(dep_uri, &file_symbols);
    let hash = ref_cache::content_hash(&rope);
    SCOPE_RESOLVER.update_file(dep_uri, hash, entries);
    FILE_SYMBOLS.insert(dep_uri.clone(), file_symbols);
}

/// Compute content hash for a URI — reads from ROPE_MAP if available,
/// otherwise reads from disk.
fn compute_hash_for_uri(uri: &Url) -> [u8; 32] {
    if let Some(rope_entry) = ROPE_MAP.get(uri) {
        return ref_cache::content_hash(rope_entry.value());
    }
    if let Ok(path) = uri.to_file_path() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            let rope = Rope::from(content.as_str());
            return ref_cache::content_hash(&rope);
        }
    }
    [0u8; 32]
}

/// Convert `FileSymbols` into `GlobalEntry` items for the scope resolver.
fn file_symbols_to_entries(
    uri: &Url,
    fs: &crate::lng::jass::symbol::FileSymbols,
) -> Vec<GlobalEntry> {
    let mut entries = Vec::new();

    for f in &fs.functions {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: f.name.clone(),
            ns: SymbolNS::Func,
            decl_key: 0, // updated when RefMap is available
            type_name: None,
            params: f.params.iter().map(|p| (p.name.clone(), p.type_name.clone())).collect(),
            return_type: f.return_type.clone(),
            is_constant: false,
            is_array: false,
        });
    }
    for n in &fs.natives {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: n.name.clone(),
            ns: SymbolNS::Func,
            decl_key: 0,
            type_name: None,
            params: n.params.iter().map(|p| (p.name.clone(), p.type_name.clone())).collect(),
            return_type: n.return_type.clone(),
            is_constant: false,
            is_array: false,
        });
    }
    for g in &fs.globals {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: g.name.clone(),
            ns: SymbolNS::Var,
            decl_key: 0,
            type_name: g.type_name.clone(),
            params: vec![],
            return_type: None,
            is_constant: g.is_constant,
            is_array: g.is_array,
        });
    }
    for t in &fs.types {
        entries.push(GlobalEntry {
            uri: uri.clone(),
            name: t.name.clone(),
            ns: SymbolNS::Var,
            decl_key: 0,
            type_name: None,
            params: vec![],
            return_type: None,
            is_constant: false,
            is_array: false,
        });
    }

    entries
}

/// Parse and store all LSP data for `uri`.
///
/// Returns the list of **open** files that directly import `uri` and should be
/// re-parsed to pick up the new symbols.
pub async fn parse(uri: &Url) -> Result<Vec<Url>, Box<dyn Error + Send + Sync>> {
    let result = _parse(uri);
    uri_unlock(uri);
    result
}

fn _parse(uri: &Url) -> Result<Vec<Url>, Box<dyn Error + Send + Sync>> {
    let rope_entry = ROPE_MAP.get(&uri.clone()).ok_or("no rope")?;
    let rope = rope_entry.value();

    let tree_entry = TREE_MAP.get(&uri.clone()).ok_or("no tree")?;
    let root = tree_entry.value().root_node();

    // 1. Build AST from CST
    let mut ast = build_ast(root);

    // 2. Rewrite leading `//import` / `//import!` comments into Import nodes
    let src: Vec<u8> = rope.slice_to_cow(0..rope.len()).as_bytes().to_vec();
    rewrite_imports(&mut ast, &src);

    // 3. Extract resolved import URLs, document links, and diagnostics
    //    for non-existent import paths.
    let mut imports = HashSet::new();
    let mut frozen_imports = HashSet::new();
    let mut links = Vec::new();
    let mut import_diagnostics = Vec::new();

    for item in &ast.items {
        if let Statement::Import(imp) = item {
            if imp.path.is_empty() {
                continue; // cursor already emits "Missing import path"
            }

            // Compute the range of the path portion within the import node.
            let node = &imp.node;
            let prefix_len = if imp.frozen {
                "//import!".len()
            } else {
                "//import".len()
            };
            let node_text = std::str::from_utf8(
                &src[node.start_byte()..node.end_byte()],
            )
            .unwrap_or("");
            let after_prefix = &node_text[prefix_len..];
            let ws_len = after_prefix.len() - after_prefix.trim_start().len();
            let path_start_byte = node.start_byte() + prefix_len + ws_len;
            let path_end_byte = node.start_byte() + prefix_len + ws_len + imp.path.len();

            let path_range = Range {
                start: Position::from_byte_offset(rope, path_start_byte)
                    .unwrap_or_default(),
                end: Position::from_byte_offset(rope, path_end_byte)
                    .unwrap_or_default(),
            };

            match resolve_import(uri, &imp.path) {
                Some(resolved) => {
                    imports.insert(resolved.url.clone());

                    if imp.frozen {
                        frozen_imports.insert(resolved.url.clone());
                    }

                    if resolved.exists {
                        // Clickable link → opens the file
                        links.push(DocumentLink {
                            range: path_range,
                            target: Some(resolved.url.to_string()),
                            tooltip: Some(resolved.url.to_string()),
                        });
                    } else {
                        // File does not exist → diagnostic error
                        import_diagnostics.push(Diagnostic {
                            range: path_range,
                            message: format!("File not found: {}", imp.path),
                            severity: Some(DiagnosticSeverity::Error),
                            ..Default::default()
                        });
                    }
                }
                None => {
                    import_diagnostics.push(Diagnostic {
                        range: path_range,
                        message: format!("Cannot resolve import path: {}", imp.path),
                        severity: Some(DiagnosticSeverity::Error),
                        ..Default::default()
                    });
                }
            }
        }
    }
    // Capture the old connected component BEFORE updating the graph,
    // so we can detect peers that lost visibility after import removal.
    let old_component = IMPORT_GRAPH.connected_component(uri);

    IMPORT_GRAPH.update(uri, imports.clone());

    // 4. Gather symbols from the **entire connected component** (unified scope).
    //    All files connected via imports — in either direction — share one
    //    global namespace, so a file sees symbols from every peer, not just
    //    its direct imports.
    let mut imported_symbols: Vec<ImportedSymbol> = Vec::new();
    let component: HashSet<Url>;
    {
        component = IMPORT_GRAPH.connected_component(uri);

        // Ensure every peer is in the scope resolver.
        for peer_uri in &component {
            ensure_file_symbols(peer_uri);
        }

        // O(1)-per-name: get all symbols from the connected component.
        let visible_entries = SCOPE_RESOLVER.all_visible(&component);

        for entry in &visible_entries {
            let origin_ref_map = REF_URI_MAP.get(&entry.uri);
            let origin_decl_key = origin_ref_map
                .as_ref()
                .and_then(|rm| find_decl_key_by_name(rm.value(), &entry.name));

            imported_symbols.push(ImportedSymbol {
                origin_uri: entry.uri.clone(),
                name: entry.name.clone(),
                kind: match entry.ns {
                    SymbolNS::Func => ImportedKind::Func,
                    SymbolNS::Var => ImportedKind::Var,
                },
                origin_decl_key,
            });
        }
    }

    // 5. Single-pass cursor: diagnostics + symbols + folding + id_roles + scopes
    let mut cursor = Cursor::walk(&ast, rope, &imported_symbols);
    cursor.file_symbols.frozen_imports = frozen_imports;
    cursor.file_symbols.file_settings = cursor.file_settings.clone();

    // 6. Merge import diagnostics with cursor diagnostics
    let mut all_diagnostics = cursor.diagnostics;
    all_diagnostics.extend(import_diagnostics);

    // 7. Store results (diagnostic report deferred until step 8)
    FOLDING_URI_MAP.insert(uri.clone(), cursor.folding);
    SYMBOL_URI_MAP.insert(uri.clone(), cursor.symbols);
    SEMANTIC_URI_MAP.insert(uri.clone(), cursor.semantic);
    LINK_URI_MAP.insert(uri.clone(), links);
    FILE_SYMBOLS.insert(uri.clone(), cursor.file_symbols);
    // Persist FileSymbols to disk cache.
    if let Some(meta) = symbol_cache::FileMeta::from_uri(uri) {
        symbol_cache::store(uri, meta, &FILE_SYMBOLS.get(uri).unwrap().value().clone());
    }
    let func_decl_keys = cursor.func_decl_keys;
    let ref_map = build_ref_map(cursor.ref_groups, cursor.ref_names, cursor.external_decls, rope);
    // Persist to disk cache for fast reload on restart.
    let hash = ref_cache::content_hash(rope);
    ref_cache::store(uri, &hash, &ref_map);

    // 8. Call-graph diagnostics: unused functions & cyclic calls.
    //    Must run after FILE_SYMBOLS is stored for the current file.
    //    Only target ref_map groups whose DeclKey belongs to a function
    //    declaration (not a same-named variable).
    {
        use crate::util::call_graph::diagnose_functions;

        let func_diag = diagnose_functions(uri);

        for (&key, group) in &ref_map.groups {
            if !func_decl_keys.contains(&key) {
                continue;
            }
            if let Some(decl_occ) = group.occurrences.iter().find(|o| o.is_decl) {
                if func_diag.unused.contains(&group.name) {
                    all_diagnostics.push(Diagnostic {
                        range: decl_occ.range.clone(),
                        message: format!("Unused function `{}`", group.name),
                        severity: Some(DiagnosticSeverity::Hint),
                        tags: Some(vec![crate::lsp::diagnostic::lsp::DiagnosticTag::Unnecessary]),
                        ..Default::default()
                    });
                }
                if func_diag.in_cycle.contains(&group.name) {
                    all_diagnostics.push(Diagnostic {
                        range: decl_occ.range.clone(),
                        message: format!(
                            "Function `{}` is part of a cyclic call chain — cannot be ordered",
                            group.name
                        ),
                        severity: Some(DiagnosticSeverity::Warning),
                        ..Default::default()
                    });
                }
            }
        }
    }

    // 9. Store diagnostics and ref_map.
    DIAGNOSTIC_URI_MAP.insert(uri.clone(), DocumentDiagnosticReport::Full {
        result_id: None,
        items: all_diagnostics,
        related_documents: None,
    });
    REF_URI_MAP.insert(uri.clone(), ref_map);

    // Update scope resolver — compare fingerprints to detect export changes.
    let old_fp = SCOPE_RESOLVER.export_fingerprint(uri);
    {
        let fs = FILE_SYMBOLS.get(uri).unwrap();
        let entries = file_symbols_to_entries(uri, fs.value());
        SCOPE_RESOLVER.update_file(uri, hash, entries);
    }
    let new_fp = SCOPE_RESOLVER.export_fingerprint(uri);

    // If exported symbols changed OR the connected component changed
    // (imports were added/removed), return the union of old and new peers
    // that are currently open so the caller can cascade re-parse them.
    let cascade = if old_fp != new_fp || old_component != component {
        let mut all_affected = old_component;
        all_affected.extend(component.into_iter());
        all_affected
            .into_iter()
            .filter(|peer| peer != uri && ROPE_MAP.contains_key(peer))
            .collect()
    } else {
        vec![]
    };

    Ok(cascade)
}
