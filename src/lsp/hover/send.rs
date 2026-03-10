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

/// Pick the best doc by the system locale env vars.
/// Falls back to English.
fn import_doc() -> &'static str {
    let lang = std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANGUAGE"))
        .unwrap_or_default()
        .to_lowercase();

    if lang.starts_with("ru") {
        IMPORT_RU
    } else if lang.starts_with("uk") {
        IMPORT_UK
    } else {
        IMPORT_EN
    }
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

    // Check if cursor is on an //import or //import! directive
    let (prefix, _frozen) = if trimmed.starts_with("//import!") {
        ("//import!", true)
    } else if trimmed.starts_with("//import")
        && (trimmed.len() == 8
            || trimmed.as_bytes()[8] == b' '
            || trimmed.as_bytes()[8] == b'\t')
    {
        ("//import", false)
    } else {
        return None;
    };

    // The prefix starts at the beginning of trimmed text
    let leading_ws = line_text.len() - trimmed.len();
    let prefix_start_col = leading_ws;
    let prefix_end_col = leading_ws + prefix.len();

    // Only trigger hover when cursor is on the //import(!) keyword itself
    let col = position.character;
    if col < prefix_start_col || col > prefix_end_col {
        return None;
    }

    Some(Hover {
        contents: MarkupContent {
            kind: MarkupKind::Markdown,
            value: import_doc().to_string(),
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

