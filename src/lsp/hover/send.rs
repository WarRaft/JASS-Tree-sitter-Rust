use crate::lsp::cancel::CancelId;
use crate::lsp::hover::lsp::{Hover, MarkupContent, MarkupKind};
use crate::lsp::position::Position;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::range::Range;
use crate::lsp::send::send as lsp_send;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::uri_map::LNG_URI_MAP;
use serde_json::Value;
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;
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

/// Pick the best doc by the system locale env vars.
/// Falls back to English.
fn pick_locale<F: Fn(&str) -> &'static str>(picker: F) -> &'static str {
    let lang = std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANGUAGE"))
        .unwrap_or_default()
        .to_lowercase();

    if lang.starts_with("ru") {
        picker("ru")
    } else if lang.starts_with("uk") {
        picker("uk")
    } else if lang.starts_with("zh_tw") || lang.starts_with("zh_hant") || lang.starts_with("zh-tw") || lang.starts_with("zh-hant") {
        picker("tc")
    } else if lang.starts_with("zh") {
        picker("zh")
    } else {
        picker("en")
    }
}

fn import_doc() -> &'static str {
    pick_locale(|l| match l {
        "ru" => IMPORT_RU,
        "uk" => IMPORT_UK,
        "zh" => IMPORT_ZH,
        "tc" => IMPORT_TC,
        _ => IMPORT_EN,
    })
}

fn set_doc() -> &'static str {
    pick_locale(|l| match l {
        "ru" => SET_RU,
        "uk" => SET_UK,
        "zh" => SET_ZH,
        "tc" => SET_TC,
        _ => SET_EN,
    })
}

// ─── Handler ─────────────────────────────────────────────────────────────────

pub async fn send(
    writer: &Arc<Mutex<Stdout>>,
    id: Option<CancelId>,
    uri: &Url,
    position: &Position,
) {
    let result = compute(uri, position);

    lsp_send(
        writer,
        &ResponseMessage::<Value> {
            jsonrpc: "2.0".into(),
            id,
            result: Some(match result {
                Some(h) => serde_json::to_value(h).unwrap_or(Value::Null),
                None => Value::Null,
            }),
            error: None,
        },
    )
    .await;
}

fn compute(uri: &Url, position: &Position) -> Option<Hover> {
    // //import hover is JASS-only
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

    // Check if cursor is on an //import, //import!, or //set directive
    let (prefix, doc_fn): (&str, fn() -> &'static str) =
        if trimmed.starts_with("//import!") {
            ("//import!", import_doc as fn() -> &'static str)
        } else if trimmed.starts_with("//import")
            && (trimmed.len() == 8
                || trimmed.as_bytes()[8] == b' '
                || trimmed.as_bytes()[8] == b'\t')
        {
            ("//import", import_doc as fn() -> &'static str)
        } else if trimmed.starts_with("//set")
            && (trimmed.len() == 5
                || trimmed.as_bytes()[5] == b' '
                || trimmed.as_bytes()[5] == b'\t')
        {
            ("//set", set_doc as fn() -> &'static str)
        } else {
            return None;
        };

    // The prefix starts at the beginning of trimmed text
    let leading_ws = line_text.len() - trimmed.len();
    let prefix_start_col = leading_ws;
    let prefix_end_col = leading_ws + prefix.len();
    let col = position.character;

    // ── Per-key hover for //set directives ─────────────────────────────
    if prefix == "//set" {
        let after_set = &trimmed["//set".len()..];
        let key_part = after_set.trim_start();
        let ws_before_key = after_set.len() - key_part.len();

        // Extract key
        let key_len = key_part
            .find(|c: char| c == ' ' || c == '\t')
            .unwrap_or(key_part.len());

        if key_len > 0 {
            let key_start_col = prefix_start_col + "//set".len() + ws_before_key;
            let key_end_col = key_start_col + key_len;
            let key = &key_part[..key_len];

            // Cursor on key → show per-key docs
            if col >= key_start_col && col <= key_end_col {
                if let Some(def) = crate::lng::directive::find_set_def(key) {
                    let type_label = match def.kind {
                        crate::lng::directive::SetValueKind::Bool => "`0` | `1`",
                        crate::lng::directive::SetValueKind::Path => "`<path>`",
                    };
                    let md = format!(
                        "### `//set {}`\n\n{}\n\n**Type:** {}\\\n**Default:** `{}`",
                        def.key, def.detail, type_label, def.default
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

