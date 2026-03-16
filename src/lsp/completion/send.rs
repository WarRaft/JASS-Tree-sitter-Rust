use crate::lsp::cancel::CancelId;
use crate::lsp::completion::lsp::{
    CompletionItem, CompletionItemKind, CompletionList, InsertTextFormat,
};
use crate::lsp::position::Position;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use crate::util::roper::uri_map::ROPE_MAP;
use std::path::Path;
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;
use url::Url;

use crate::lng::jass::kind::{Field, Kind};
use crate::util::file_store::FILE_STORE;
use crate::util::import_graph::IMPORT_GRAPH;
use crate::util::scope_resolver::{SCOPE_RESOLVER, SymbolNS};
use crate::util::tree_map::TREE_MAP;
use crate::util::uri_map::LNG_URI_MAP;

/// Handle `textDocument/completion`.
pub async fn send(
    writer: &Arc<Mutex<Stdout>>,
    id: Option<CancelId>,
    uri: &Url,
    position: &Position,
) {
    let items = compute(uri, position);

    lsp_send(
        writer,
        &ResponseMessage {
            jsonrpc: "2.0".into(),
            id,
            result: Some(CompletionList {
                is_incomplete: items
                    .iter()
                    .any(|i| i.kind == Some(CompletionItemKind::Folder)),
                items,
            }),
            error: None,
        },
    )
    .await;
}

fn compute(uri: &Url, position: &Position) -> Vec<CompletionItem> {
    let rope_entry = match ROPE_MAP.get(uri) {
        Some(e) => e,
        None => return vec![],
    };
    let rope = rope_entry.value();

    // Extract line text up to cursor position.
    let line_idx = position.line;
    if line_idx >= rope.line_of_offset(rope.len()) + 1 {
        return vec![];
    }
    let line_start = rope.offset_of_line(line_idx);
    let line_end = rope.offset_of_line(line_idx + 1).min(rope.len());
    let line_text = rope.slice_to_cow(line_start..line_end).to_string();

    // The prefix is everything up to the cursor column.
    // LSP position.character is in UTF-16 code units; convert to byte offset.
    let byte_col = utf16_to_byte_offset(&line_text, position.character);
    let prefix = &line_text[..byte_col.min(line_text.len())];

    // ── Case 1: cursor right after "//" → suggest `import`, `import!`, `set`, `@ignore` ──
    if prefix.trim_start() == "//" {
        return vec![
            CompletionItem {
                label: "import".into(),
                kind: Some(CompletionItemKind::Keyword),
                detail: Some("Import a file".into()),
                insert_text: Some("import ".into()),
                sort_text: Some("0".into()),
                ..Default::default()
            },
            CompletionItem {
                label: "import!".into(),
                kind: Some(CompletionItemKind::Keyword),
                detail: Some("Import a frozen (read-only) file".into()),
                insert_text: Some("import! ".into()),
                sort_text: Some("1".into()),
                ..Default::default()
            },
            CompletionItem {
                label: "set".into(),
                kind: Some(CompletionItemKind::Keyword),
                detail: Some("Set a file-local configuration value".into()),
                insert_text: Some("set ".into()),
                sort_text: Some("2".into()),
                ..Default::default()
            },
            CompletionItem {
                label: "@ignore".into(),
                kind: Some(CompletionItemKind::Keyword),
                detail: Some("Suppress diagnostics for the next declaration".into()),
                insert_text: Some("@ignore ".into()),
                sort_text: Some("3".into()),
                ..Default::default()
            },
        ];
    }

    // ── Case 1b: cursor after "//@" → suggest `ignore` ──────────────────────────
    if prefix.trim_start() == "//@" {
        return vec![CompletionItem {
            label: "ignore".into(),
            kind: Some(CompletionItemKind::Keyword),
            detail: Some("Suppress diagnostics for the next declaration".into()),
            insert_text: Some("ignore ".into()),
            sort_text: Some("0".into()),
            ..Default::default()
        }];
    }

    // ── Case 2: cursor after "//set " → suggest known setting keys ─────────────
    let trimmed = prefix.trim_start();

    // ── Case 2b: cursor after "//@ignore " → suggest known tags ────────────────
    if let Some(rest) = trimmed.strip_prefix("//@ignore") {
        if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
            let typed = rest.trim_start();
            // Collect already-typed tags so we don't suggest them again.
            let used: std::collections::HashSet<&str> = typed.split_whitespace().collect();
            let tags: &[(&str, &str)] = &[
                ("unused", "Suppress unused-function diagnostic"),
                ("cycle", "Suppress cyclic-call-chain diagnostic"),
            ];
            return tags
                .iter()
                .filter(|(tag, _)| !used.contains(tag))
                .enumerate()
                .map(|(i, (tag, detail))| CompletionItem {
                    label: (*tag).into(),
                    kind: Some(CompletionItemKind::EnumMember),
                    detail: Some((*detail).into()),
                    insert_text: None,
                    sort_text: Some(i.to_string()),
                    ..Default::default()
                })
                .collect();
        }
    }

    if let Some(rest) = trimmed.strip_prefix("//set") {
        if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
            let typed = rest.trim_start();

            // Sub-case A: user typed "//set key " → suggest values
            if let Some(space_pos) = typed.find(|c: char| c == ' ' || c == '\t') {
                let key = &typed[..space_pos];
                if let Some(def) = crate::lng::directive::find_set_def(key) {
                    return match def.kind {
                        crate::lng::directive::SetValueKind::Bool => vec![
                            CompletionItem {
                                label: "1".into(),
                                kind: Some(CompletionItemKind::Value),
                                detail: Some("Enable".into()),
                                insert_text: Some("1".into()),
                                sort_text: Some("0".into()),
                                ..Default::default()
                            },
                            CompletionItem {
                                label: "0".into(),
                                kind: Some(CompletionItemKind::Value),
                                detail: Some("Disable".into()),
                                insert_text: Some("0".into()),
                                sort_text: Some("1".into()),
                                ..Default::default()
                            },
                        ],
                        crate::lng::directive::SetValueKind::Path => {
                            // Delegate to the existing path completion logic below
                            let path_typed = typed[space_pos..].trim_start();
                            return complete_path(uri, path_typed);
                        }
                    };
                }
                return vec![];
            }

            // Sub-case B: user is typing the key → suggest all known keys
            if !typed.contains(' ') && !typed.contains('\t') {
                use crate::lng::directive::{SET_DEFS, SetValueKind};
                return SET_DEFS
                    .iter()
                    .map(|def| {
                        let insert = match def.kind {
                            SetValueKind::Bool => format!("{} {}", def.key, def.default),
                            SetValueKind::Path => format!("{} {}", def.key, def.default),
                        };
                        CompletionItem {
                            label: def.key.into(),
                            kind: Some(CompletionItemKind::Property),
                            detail: Some(def.detail.into()),
                            insert_text: Some(insert),
                            sort_text: Some(def.sort_order.to_string()),
                            ..Default::default()
                        }
                    })
                    .collect();
            }
            return vec![];
        }
    }

    // ── Case 3: cursor on the path after "//import " or "//import! " ─────────

    let path_part = if let Some(rest) = trimmed.strip_prefix("//import!") {
        rest.strip_prefix(' ').or(Some(rest))
    } else if let Some(rest) = trimmed.strip_prefix("//import") {
        // Guard against "//importing" etc.
        if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
            Some(rest.trim_start())
        } else {
            None
        }
    } else {
        None
    };

    let path_part = match path_part {
        Some(p) => p,
        None => return complete_jass_symbols(uri, position),
    };

    complete_path(uri, path_part)
}

// ─── JASS symbol completion ──────────────────────────────────────────────────

/// Default value for a JASS type used in completion snippets.
fn default_value_for_type(type_name: &str) -> &'static str {
    match type_name {
        "integer" => "0",
        "real" => "0.",
        "boolean" => "false",
        "string" => "\"\"",
        _ => "null",
    }
}

/// Build a snippet `insertText` for a function call.
///
/// - With params: `FuncName(${1:0}, ${2:null})` — tab stops on each arg
/// - Without params: `FuncName() $0` — cursor after `() `
fn build_call_snippet(name: &str, params: &[(String, String)]) -> String {
    if params.is_empty() {
        format!("{}() $0", name)
    } else {
        let args: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(i, (_pname, ptype))| format!("${{{}:{}}}", i + 1, default_value_for_type(ptype)))
            .collect();
        format!("{}({})", name, args.join(", "))
    }
}

/// Create a [`CompletionItem`] for a function or native.
fn make_func_item(
    name: &str,
    params: &[(String, String)],
    return_type: Option<&str>,
    sort_prefix: &str,
) -> CompletionItem {
    let params_str = if params.is_empty() {
        "nothing".to_string()
    } else {
        params
            .iter()
            .map(|(pname, ptype)| format!("{} {}", ptype, pname))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let ret = return_type.unwrap_or("nothing");
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::Function),
        detail: Some(format!("takes {} returns {}", params_str, ret)),
        insert_text: Some(build_call_snippet(name, params)),
        insert_text_format: Some(InsertTextFormat::Snippet),
        sort_text: Some(format!("{}{}", sort_prefix, name)),
    }
}

/// Completion for JASS symbols: locals/args (top), then globals, functions,
/// natives, and types from the current file and its connected component.
fn complete_jass_symbols(uri: &Url, position: &Position) -> Vec<CompletionItem> {
    // Only for JASS files.
    let lng = match LNG_URI_MAP.get(uri) {
        Some(l) if l.value() == "jass" => l,
        _ => return vec![],
    };
    drop(lng);

    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // ── Locals & params (if cursor is inside a function) ─────────────────
    let inside_function;
    if let Some(tree_entry) = TREE_MAP.get(uri) {
        let tree = tree_entry.value();
        let point = tree_sitter::Point {
            row: position.line,
            column: position.character,
        };
        let root = tree.root_node();

        // Find the enclosing FunctionStatement node.
        if let Some(func_node) = find_enclosing_function(root, point) {
            inside_function = true;
            // Collect parameters.
            let params_node = func_node.child_by_field_id(Field::Parameters as u16);
            if let Some(params_node) = params_node {
                if let Some(rope_entry) = ROPE_MAP.get(uri) {
                    let rope = rope_entry.value();
                    for i in 0..params_node.named_child_count() {
                        if let Some(param) = params_node.named_child(i as u32) {
                            if param.kind_id() == Kind::Parameter as u16 {
                                let type_node = param.child_by_field_id(Field::Type as u16);
                                let name_node = param.child_by_field_id(Field::Name as u16);
                                if let (Some(tn), Some(nn)) = (type_node, name_node) {
                                    let pname = node_text_from_rope(rope, &nn);
                                    let ptype = node_text_from_rope(rope, &tn);
                                    if !pname.is_empty() && seen.insert(pname.clone()) {
                                        items.push(CompletionItem {
                                            label: pname.clone(),
                                            kind: Some(CompletionItemKind::Variable),
                                            detail: Some(ptype),
                                            insert_text: None,
                                            sort_text: Some(format!("0{}", pname)),
                                            ..Default::default()
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Collect all local variables from the function.
            if let Some(rope_entry) = ROPE_MAP.get(uri) {
                let rope = rope_entry.value();
                collect_locals(&func_node, rope, &mut items, &mut seen);
            }
        } else {
            inside_function = false;
        }
    } else {
        inside_function = false;
    }

    // ── Keyword snippets (only at top level) ─────────────────────────────
    if !inside_function {
        items.push(CompletionItem {
            label: "function".into(),
            kind: Some(CompletionItemKind::Snippet),
            detail: Some("function … endfunction".into()),
            insert_text: Some(
                "function ${1:name} takes nothing returns nothing\n\t$0\nendfunction".into(),
            ),
            insert_text_format: Some(InsertTextFormat::Snippet),
            sort_text: Some("0function".into()),
            ..Default::default()
        });
    }

    // ── Current file symbols ─────────────────────────────────────────────
    if let Some(snap_entry) = FILE_STORE.get(uri) {
        let fs = &snap_entry.value().file_symbols;

        // Functions
        for f in &fs.functions {
            if seen.insert(f.name.clone()) {
                let params: Vec<(String, String)> = f
                    .params
                    .iter()
                    .map(|p| (p.name.clone(), p.type_name.clone()))
                    .collect();
                items.push(make_func_item(
                    &f.name,
                    &params,
                    f.return_type.as_deref(),
                    "1",
                ));
            }
        }

        // Natives
        for n in &fs.natives {
            if seen.insert(n.name.clone()) {
                let params: Vec<(String, String)> = n
                    .params
                    .iter()
                    .map(|p| (p.name.clone(), p.type_name.clone()))
                    .collect();
                items.push(make_func_item(
                    &n.name,
                    &params,
                    n.return_type.as_deref(),
                    "1",
                ));
            }
        }

        // Global variables
        for g in &fs.globals {
            if seen.insert(g.name.clone()) {
                let mut detail_parts = Vec::new();
                if g.is_constant {
                    detail_parts.push("constant");
                }
                if let Some(ref tn) = g.type_name {
                    detail_parts.push(tn);
                }
                if g.is_array {
                    detail_parts.push("array");
                }
                items.push(CompletionItem {
                    label: g.name.clone(),
                    kind: Some(if g.is_constant {
                        CompletionItemKind::Constant
                    } else {
                        CompletionItemKind::Variable
                    }),
                    detail: Some(detail_parts.join(" ")),
                    insert_text: None,
                    sort_text: Some(format!("1{}", g.name)),
                    ..Default::default()
                });
            }
        }

        // Types
        for t in &fs.types {
            if seen.insert(t.name.clone()) {
                let detail = t
                    .base
                    .as_ref()
                    .map(|b| format!("extends {}", b))
                    .unwrap_or_default();
                items.push(CompletionItem {
                    label: t.name.clone(),
                    kind: Some(CompletionItemKind::Class),
                    detail: if detail.is_empty() {
                        None
                    } else {
                        Some(detail)
                    },
                    insert_text: None,
                    sort_text: Some(format!("2{}", t.name)),
                    ..Default::default()
                });
            }
        }
    }

    // ── Imported symbols (connected component) ───────────────────────────
    {
        let component = IMPORT_GRAPH.connected_component(uri);
        if !component.is_empty() {
            let mut visible = component;
            visible.insert(uri.clone());
            let entries = SCOPE_RESOLVER.all_visible(&visible);

            for entry in &entries {
                // Skip own file — already added above.
                if &entry.uri == uri {
                    continue;
                }
                if !seen.insert(entry.name.clone()) {
                    continue;
                }
                match entry.ns {
                    SymbolNS::Func => {
                        items.push(make_func_item(
                            &entry.name,
                            &entry.params,
                            entry.return_type.as_deref(),
                            "1",
                        ));
                    }
                    SymbolNS::Var => {
                        let mut detail_parts = Vec::new();
                        if entry.is_constant {
                            detail_parts.push("constant".to_string());
                        }
                        if let Some(ref tn) = entry.type_name {
                            detail_parts.push(tn.clone());
                        }
                        if entry.is_array {
                            detail_parts.push("array".to_string());
                        }
                        // Check if this is actually a type declaration
                        let detail_str = detail_parts.join(" ");
                        items.push(CompletionItem {
                            label: entry.name.clone(),
                            kind: Some(if entry.is_constant {
                                CompletionItemKind::Constant
                            } else if entry.type_name.is_none()
                                && !entry.is_array
                                && entry.params.is_empty()
                            {
                                CompletionItemKind::Class
                            } else {
                                CompletionItemKind::Variable
                            }),
                            detail: if detail_str.is_empty() {
                                None
                            } else {
                                Some(detail_str)
                            },
                            insert_text: None,
                            sort_text: Some(format!("1{}", entry.name)),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }

    items
}

/// Walk tree-sitter nodes up from the deepest node at `point` to find an
/// enclosing `FunctionStatement`.
fn find_enclosing_function(
    root: tree_sitter::Node,
    point: tree_sitter::Point,
) -> Option<tree_sitter::Node> {
    let mut node = root.descendant_for_point_range(point, point)?;
    loop {
        if node.kind_id() == Kind::FunctionStatement as u16 {
            return Some(node);
        }
        node = node.parent()?;
    }
}

/// Collect **all** `local` variable declarations from a function body.
///
/// JASS locals are hoisted to the top of the function during build, so they
/// can be referenced before their textual declaration — just like globals.
fn collect_locals(
    func_node: &tree_sitter::Node,
    rope: &lapce_xi_rope::Rope,
    items: &mut Vec<CompletionItem>,
    seen: &mut std::collections::HashSet<String>,
) {
    let child_count = func_node.child_count();
    for i in 0..child_count {
        if let Some(child) = func_node.child(i as u32) {
            if child.kind_id() == Kind::LocalStatement as u16 {
                let type_node = child.child_by_field_id(Field::Type as u16);
                let name_node = child.child_by_field_id(Field::Name as u16);
                if let (Some(tn), Some(nn)) = (type_node, name_node) {
                    let lname = node_text_from_rope(rope, &nn);
                    let ltype = node_text_from_rope(rope, &tn);
                    if !lname.is_empty() && seen.insert(lname.clone()) {
                        items.push(CompletionItem {
                            label: lname.clone(),
                            kind: Some(CompletionItemKind::Variable),
                            detail: Some(format!("local {}", ltype)),
                            insert_text: None,
                            sort_text: Some(format!("0{}", lname)),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }
}

/// Extract the text of a tree-sitter node from a Rope.
fn node_text_from_rope(rope: &lapce_xi_rope::Rope, node: &tree_sitter::Node) -> String {
    let start = node.start_byte();
    let end = node.end_byte().min(rope.len());
    if start >= end {
        return String::new();
    }
    rope.slice_to_cow(start..end).to_string()
}

/// Generate file/folder completion items for a partial `path_typed` relative
/// to the parent directory of `uri`.
fn complete_path(uri: &Url, path_typed: &str) -> Vec<CompletionItem> {
    let base_path = match uri.to_file_path() {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    let base_dir = base_path.parent().unwrap_or(Path::new("/"));

    // Normalise the partial path for filesystem lookup.
    let normalised = path_typed.replace('\\', "/");

    // Split into directory part and filename prefix.
    let (dir_part, file_prefix) = match normalised.rfind('/') {
        Some(pos) => (&normalised[..=pos], &normalised[pos + 1..]),
        None => ("", normalised.as_str()),
    };

    // Build the lookup directory.
    let lookup_dir = if dir_part.is_empty() {
        base_dir.to_path_buf()
    } else {
        base_dir.join(dir_part.replace('\\', "/"))
    };

    // Read directory entries.
    let entries = match std::fs::read_dir(&lookup_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    let mut items = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();

        // Filter by prefix.
        if !name.starts_with(file_prefix) {
            continue;
        }

        // Skip hidden files.
        if name.starts_with('.') {
            continue;
        }

        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);

        items.push(CompletionItem {
            label: if is_dir {
                format!("{}/", name)
            } else {
                name.clone()
            },
            kind: Some(if is_dir {
                CompletionItemKind::Folder
            } else {
                CompletionItemKind::File
            }),
            detail: None,
            insert_text: Some(if is_dir { format!("{}/", name) } else { name }),
            // Sort folders before files.
            sort_text: Some(format!(
                "{}{}",
                if is_dir { "0" } else { "1" },
                entry.file_name().to_string_lossy()
            )),
            ..Default::default()
        });
    }

    items
}

/// Convert a UTF-16 column offset to a byte offset in a UTF-8 string.
fn utf16_to_byte_offset(text: &str, utf16_col: usize) -> usize {
    let mut utf16_count = 0;
    for (byte_idx, ch) in text.char_indices() {
        if utf16_count >= utf16_col {
            return byte_idx;
        }
        utf16_count += ch.len_utf16();
    }
    text.len()
}
