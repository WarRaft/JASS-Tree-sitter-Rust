use crate::lsp::hover::lsp::{Hover, MarkupContent, MarkupKind};
use crate::lsp::position::Position;
use crate::lsp::range::Range;
use std::sync::Arc;
use crate::util::file_store::FILE_STORE;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::scope_resolver::{SymbolNS, SCOPE_RESOLVER};
use crate::util::uri_map::LNG_URI_MAP;
use url::Url;

// ─── Embedded docs (JASS only) ──────────────────────────────────────────────

const IMPORT_EN: &str = include_str!("../../../docs/jass/import/en.md");
const IMPORT_RU: &str = include_str!("../../../docs/jass/import/ru.md");
const IMPORT_UK: &str = include_str!("../../../docs/jass/import/uk.md");
const IMPORT_ZH: &str = include_str!("../../../docs/jass/import/zh.md");
const IMPORT_TC: &str = include_str!("../../../docs/jass/import/tc.md");

const SET_EN: &str = include_str!("../../../docs/jass/set/en.md");
const SET_RU: &str = include_str!("../../../docs/jass/set/ru.md");
const SET_UK: &str = include_str!("../../../docs/jass/set/uk.md");
const SET_ZH: &str = include_str!("../../../docs/jass/set/zh.md");
const SET_TC: &str = include_str!("../../../docs/jass/set/tc.md");

const IGNORE_EN: &str = include_str!("../../../docs/jass/ignore/en.md");
const IGNORE_RU: &str = include_str!("../../../docs/jass/ignore/ru.md");
const IGNORE_UK: &str = include_str!("../../../docs/jass/ignore/uk.md");
const IGNORE_ZH: &str = include_str!("../../../docs/jass/ignore/zh.md");
const IGNORE_TC: &str = include_str!("../../../docs/jass/ignore/tc.md");

fn import_doc() -> &'static str {
    crate::util::i18n::pick(IMPORT_EN, IMPORT_RU, IMPORT_UK, IMPORT_ZH, IMPORT_TC)
}

fn set_doc() -> &'static str {
    crate::util::i18n::pick(SET_EN, SET_RU, SET_UK, SET_ZH, SET_TC)
}

fn ignore_doc() -> &'static str {
    crate::util::i18n::pick(IGNORE_EN, IGNORE_RU, IGNORE_UK, IGNORE_ZH, IGNORE_TC)
}

// ─── Handler ─────────────────────────────────────────────────────────────────


pub(crate) fn compute(uri: &Url, position: &Position) -> Option<Hover> {
    let lng = LNG_URI_MAP.get(uri)?;
    if lng.value() != "jass" {
        return None;
    }

    let rope_entry = ROPE_MAP.get(uri)?;
    let rope = rope_entry.value();

    let line_idx = position.line;
    let line_count = rope.line_of_offset(rope.len()) + 1;
    if line_idx >= line_count {
        return None;
    }

    let line_start = rope.offset_of_line(line_idx);
    let line_end = if line_idx + 1 < line_count {
        rope.offset_of_line(line_idx + 1)
    } else {
        rope.len()
    };
    let line_text = rope.slice_to_cow(line_start..line_end);
    let trimmed = line_text.trim_start();

    // ── Directive hover (//import, //set) ────────────────────────────────
    if let Some(hover) = compute_directive_hover(trimmed, &line_text, line_idx, position.character) {
        return Some(hover);
    }

    // ── Symbol hover (doc comments) ──────────────────────────────────────
    compute_symbol_hover(uri, position)
}

/// Try directive hover for `//import`, `//set`, and `//ignore` directives.
fn compute_directive_hover(
    trimmed: &str,
    line_text: &str,
    line_idx: usize,
    col: usize,
) -> Option<Hover> {
    // ── Special: //import-ujapi! — dynamic hover with version ─────────
    if trimmed.starts_with("//import-ujapi!") {
        let leading_ws = line_text.len() - trimmed.len();
        let prefix = "//import-ujapi!";
        let prefix_start_col = leading_ws;
        let prefix_end_col = leading_ws + prefix.len();

        if col >= prefix_start_col && col <= prefix_end_col {
            let latest = crate::util::ujapi::cached_release();
            let version_line = match &latest {
                Some(rel) => crate::util::i18n::ujapi_hover_latest_release(
                    &rel.tag, &rel.html_url, &rel.name,
                ),
                None => crate::util::i18n::ujapi_hover_fetching().to_string(),
            };

            let md = crate::util::i18n::ujapi_hover_body(&version_line);
            return Some(Hover {
                contents: MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: md,
                },
                range: Some(Range {
                    start: Position { line: line_idx, character: prefix_start_col },
                    end: Position { line: line_idx, character: prefix_end_col },
                }),
            });
        }

        return None;
    }

    let (prefix, doc_fn): (&str, fn() -> &'static str) =
        if trimmed.starts_with("//import!") {
            ("//import!", import_doc as fn() -> &'static str)
        } else if trimmed.starts_with("//import")
            && (trimmed.len() == 8
                || trimmed.as_bytes()[8] == b' '
                || trimmed.as_bytes()[8] == b'\t')
        {
            ("//import", import_doc as fn() -> &'static str)
        } else if trimmed.starts_with("//ignore")
            && (trimmed.len() == 8
                || trimmed.as_bytes()[8] == b' '
                || trimmed.as_bytes()[8] == b'\t')
        {
            ("//ignore", ignore_doc as fn() -> &'static str)
        } else if trimmed.starts_with("//set")
            && (trimmed.len() == 5
                || trimmed.as_bytes()[5] == b' '
                || trimmed.as_bytes()[5] == b'\t')
        {
            ("//set", set_doc as fn() -> &'static str)
        } else {
            return None;
        };

    let leading_ws = line_text.len() - trimmed.len();
    let prefix_start_col = leading_ws;
    let prefix_end_col = leading_ws + prefix.len();

    // ── Per-key hover for //set directives ─────────────────────────────
    if prefix == "//set" {
        let after_set = &trimmed["//set".len()..];
        let key_part = after_set.trim_start();
        let ws_before_key = after_set.len() - key_part.len();

        let key_len = key_part
            .find(|c: char| c == ' ' || c == '\t')
            .unwrap_or(key_part.len());

        if key_len > 0 {
            let key_start_col = prefix_start_col + "//set".len() + ws_before_key;
            let key_end_col = key_start_col + key_len;
            let key = &key_part[..key_len];

            if col >= key_start_col && col <= key_end_col {
                if let Some(def) = crate::lng::directive::find_set_def(key) {
                    let detail = crate::util::i18n::set_def_detail(def.key);
                    let type_label = match def.kind {
                        crate::lng::directive::SetValueKind::Bool => "`0` | `1`",
                        crate::lng::directive::SetValueKind::Path => "`<path>`",
                        crate::lng::directive::SetValueKind::Command => "`<command>`",
                    };
                    let md = format!(
                        "### `//set {}`\n\n{}\n\n**Type:** {}\\\n**Default:** `{}`",
                        def.key, detail, type_label, def.default
                    );
                    return Some(Hover {
                        contents: MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: md,
                        },
                        range: Some(Range {
                            start: Position { line: line_idx, character: key_start_col },
                            end: Position { line: line_idx, character: key_end_col },
                        }),
                    });
                }
            }

            // ── Per-{{var}} hover for Command values ─────────────────
            let is_command = matches!(
                crate::lng::directive::find_set_def(key),
                Some(def) if def.kind == crate::lng::directive::SetValueKind::Command
            );
            if is_command && key_len < key_part.len() {
                let after_key = &key_part[key_len..];
                let value_part = after_key.trim_start();
                let ws_before_value = after_key.len() - value_part.len();
                let val_start_col = key_start_col + key_len + ws_before_value;

                let spans = crate::lng::directive::find_template_spans(value_part);
                for (span_off, span_len, var_name) in &spans {
                    let var_start_col = val_start_col + span_off;
                    let var_end_col = var_start_col + span_len;
                    if col >= var_start_col && col < var_end_col {
                        let detail = crate::util::i18n::template_var_detail(var_name);
                        let md = if detail.is_empty() {
                            format!("### `{{{{{}}}}}`\n\nUnknown template variable.", var_name)
                        } else {
                            format!("### `{{{{{}}}}}`\n\n{}", var_name, detail)
                        };
                        return Some(Hover {
                            contents: MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: md,
                            },
                            range: Some(Range {
                                start: Position { line: line_idx, character: var_start_col },
                                end: Position { line: line_idx, character: var_end_col },
                            }),
                        });
                    }
                }
            }
        }
    }

    // ── Per-tag hover for //ignore directives ──────────────────────────
    if prefix == "//ignore" {
        let after_ignore = &trimmed["//ignore".len()..];
        // Walk each word-token, find which one the cursor is on
        let mut cursor_pos = 0usize;
        for tag_str in after_ignore.split_whitespace() {
            if let Some(pos) = after_ignore[cursor_pos..].find(tag_str) {
                let tag_start_col = prefix_start_col + "//ignore".len() + cursor_pos + pos;
                let tag_end_col = tag_start_col + tag_str.len();
                cursor_pos += pos + tag_str.len();
                if col >= tag_start_col && col <= tag_end_col {
                    if let Some(def) = crate::lng::directive::find_ignore_tag(tag_str) {
                        let detail = crate::util::i18n::ignore_tag_detail(def.tag);
                        let md = format!("### `//ignore {}`\n\n{}", def.tag, detail);
                        return Some(Hover {
                            contents: MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: md,
                            },
                            range: Some(Range {
                                start: Position { line: line_idx, character: tag_start_col },
                                end: Position { line: line_idx, character: tag_end_col },
                            }),
                        });
                    }
                }
            }
        }
    }

    // Cursor on the prefix keyword → show generic doc
    if col < prefix_start_col || col > prefix_end_col {
        return None;
    }

    Some(Hover {
        contents: MarkupContent {
            kind: MarkupKind::Markdown,
            value: doc_fn().to_string(),
        },
        range: Some(Range {
            start: Position {
                line: line_idx,
                character: prefix_start_col,
            },
            end: Position {
                line: line_idx,
                character: prefix_end_col,
            },
        }),
    })
}

/// Build a JASS signature string for a function/native.
fn build_signature(
    name: &str,
    params: &[(String, String)],
    return_type: &Option<String>,
) -> String {
    let params_str = if params.is_empty() {
        "nothing".to_string()
    } else {
        params
            .iter()
            .map(|(pname, ptype)| format!("{} {}", ptype, pname))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let ret = return_type.as_deref().unwrap_or("nothing");
    format!("function {} takes {} returns {}", name, params_str, ret)
}

/// Build a JASS declaration string for a global variable.
fn build_global_decl(
    name: &str,
    type_name: &Option<String>,
    is_constant: bool,
    is_array: bool,
) -> String {
    let mut parts = Vec::new();
    if is_constant {
        parts.push("constant");
    }
    if let Some(tn) = type_name {
        parts.push(tn);
    }
    if is_array {
        parts.push("array");
    }
    parts.push(name);
    parts.join(" ")
}

/// Symbol hover: look up the symbol under cursor and show its doc comment.
fn compute_symbol_hover(uri: &Url, position: &Position) -> Option<Hover> {
    let snapshot = FILE_STORE.get(uri)?;
    let snap = Arc::clone(snapshot.value());

    let rope_entry = ROPE_MAP.get(uri)?;
    let rope = rope_entry.value();

    let byte_offset = position.to_byte_offset(rope)?;
    let ref_map = &snap.ref_map;

    // Get the name and range under cursor
    let span = ref_map.spans.iter().find(|s| {
        byte_offset >= s.start_byte && byte_offset < s.end_byte
    })?;
    let name = ref_map.groups.get(&span.decl_key)?.name.as_str();
    let hover_range = span.range.clone();

    // Determine the source of the symbol — local or external
    let (doc_comment, signature) = if let Some(ext) = ref_map.external_decls.get(&span.decl_key) {
        // External symbol — try all origin files, prefer the one with a doc comment
        let mut best_doc = None;
        let mut best_sig = None;
        for origin in &ext.origins {
            let (doc, sig) = lookup_symbol_info(&origin.uri, &ext.name);
            if best_sig.is_none() {
                best_sig = sig;
            }
            if doc.is_some() {
                best_doc = doc;
                break; // found a doc comment, use it
            }
        }
        (best_doc, best_sig)
    } else {
        // Local symbol — look up in this file's FileSymbols
        lookup_symbol_info(uri, name)
    };

    // Build the hover content
    let mut md = String::new();
    if let Some(sig) = &signature {
        md.push_str("```jass\n");
        md.push_str(sig);
        md.push_str("\n```");
    }
    if let Some(doc) = &doc_comment {
        if !doc.is_empty() {
            if !md.is_empty() {
                md.push_str("\n\n---\n\n");
            }
            md.push_str(doc);
        }
    }

    if md.is_empty() {
        return None;
    }

    Some(Hover {
        contents: MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        },
        range: Some(hover_range),
    })
}

/// Look up signature and doc_comment for a symbol by URI and name.
/// Returns `(doc_comment, signature)`.
fn lookup_symbol_info(uri: &Url, name: &str) -> (Option<String>, Option<String>) {
    // Try FILE_STORE first for the most up-to-date data
    if let Some(snap_entry) = FILE_STORE.get(uri) {
        let fs = &snap_entry.value().file_symbols;
        if let Some(f) = fs.find_function(name) {
            let params: Vec<(String, String)> = f.params.iter()
                .map(|p| (p.name.clone(), p.type_name.clone()))
                .collect();
            return (
                f.doc_comment.clone(),
                Some(build_signature(name, &params, &f.return_type)),
            );
        }
        if let Some(n) = fs.find_native(name) {
            let params: Vec<(String, String)> = n.params.iter()
                .map(|p| (p.name.clone(), p.type_name.clone()))
                .collect();
            return (
                n.doc_comment.clone(),
                Some(build_signature(name, &params, &n.return_type)),
            );
        }
        if let Some(g) = fs.find_global(name) {
            return (
                g.doc_comment.clone(),
                Some(build_global_decl(name, &g.type_name, g.is_constant, g.is_array)),
            );
        }
        if let Some(t) = fs.find_type(name) {
            let sig = if let Some(ref base) = t.base {
                format!("type {} extends {}", name, base)
            } else {
                format!("type {}", name)
            };
            return (t.doc_comment.clone(), Some(sig));
        }
    }

    // Fallback: SCOPE_RESOLVER (for files not yet in FILE_STORE)
    let all_uris: std::collections::HashSet<Url> = std::iter::once(uri.clone()).collect();
    let entries = SCOPE_RESOLVER.resolve(name, SymbolNS::Func, &all_uris);
    if let Some(e) = entries.first() {
        return (
            e.doc_comment.clone(),
            Some(build_signature(name, &e.params, &e.return_type)),
        );
    }
    let entries = SCOPE_RESOLVER.resolve(name, SymbolNS::Var, &all_uris);
    if let Some(e) = entries.first() {
        return (
            e.doc_comment.clone(),
            Some(build_global_decl(name, &e.type_name, e.is_constant, e.is_array)),
        );
    }

    (None, None)
}

