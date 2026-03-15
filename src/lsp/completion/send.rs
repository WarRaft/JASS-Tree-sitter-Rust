use crate::lsp::cancel::CancelId;
use crate::lsp::completion::lsp::{
    CompletionItem, CompletionItemKind, CompletionList,
};
use crate::lsp::position::Position;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use crate::util::roper::uri_map::ROPE_MAP;
use std::path::Path;
use tokio::io::Stdout;
use tokio::sync::Mutex;
use std::sync::Arc;
use url::Url;

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
                is_incomplete: items.iter().any(|i| i.kind == Some(CompletionItemKind::Folder)),
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

    // ── Case 1: cursor right after "//" → suggest `import`, `import!`, `set` ──
    if prefix.trim_start() == "//" {
        return vec![
            CompletionItem {
                label: "import".into(),
                kind: Some(CompletionItemKind::Keyword),
                detail: Some("Import a file".into()),
                insert_text: Some("import ".into()),
                sort_text: Some("0".into()),
            },
            CompletionItem {
                label: "import!".into(),
                kind: Some(CompletionItemKind::Keyword),
                detail: Some("Import a frozen (read-only) file".into()),
                insert_text: Some("import! ".into()),
                sort_text: Some("1".into()),
            },
            CompletionItem {
                label: "set".into(),
                kind: Some(CompletionItemKind::Keyword),
                detail: Some("Set a file-local configuration value".into()),
                insert_text: Some("set ".into()),
                sort_text: Some("2".into()),
            },
        ];
    }

    // ── Case 2: cursor after "//set " → suggest known setting keys ─────────────
    let trimmed = prefix.trim_start();

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
                            },
                            CompletionItem {
                                label: "0".into(),
                                kind: Some(CompletionItemKind::Value),
                                detail: Some("Disable".into()),
                                insert_text: Some("0".into()),
                                sort_text: Some("1".into()),
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
                return SET_DEFS.iter().map(|def| {
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
                    }
                }).collect();
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
        None => return vec![],
    };

    complete_path(uri, path_part)
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

        let is_dir = entry
            .file_type()
            .map(|ft| ft.is_dir())
            .unwrap_or(false);

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
            insert_text: Some(if is_dir {
                format!("{}/", name)
            } else {
                name
            }),
            // Sort folders before files.
            sort_text: Some(format!("{}{}", if is_dir { "0" } else { "1" }, entry.file_name().to_string_lossy())),
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

