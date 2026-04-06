//! Dispatch formatting requests to the appropriate language formatter.

use crate::lsp::cancel::CancelId;
use crate::lsp::formatting::lsp::{DocumentFormattingParams, TextEdit};
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send;
use crate::util::file_store::FILE_STORE;
use crate::util::uri_map::LNG_URI_MAP;
use std::collections::HashMap;

pub async fn send_formatting(
    id: Option<CancelId>,
    params: &DocumentFormattingParams,
) {
    let uri = &params.text_document.uri;

    let edits: Vec<TextEdit> = if let Some(lng) = LNG_URI_MAP.get(uri) {
        match lng.value().as_str() {
            "jass" => crate::lsp::formatting::jass::format(uri, &params.options),
            "angelscript" => crate::lsp::formatting::ass::format(uri, &params.options),
            _ => vec![],
        }
    } else {
        vec![]
    };

    // ── Adjust semantic token positions for formatting edits ────────────
    //
    // Leading-whitespace edits shift every token on the affected line by
    // the same delta.  We apply that to the Hub now, before the client
    // applies the edits and before the didChange → re-parse cycle starts.
    //
    // Inline edits (operator spacing, commas, etc.) produce per-token
    // deltas that are harder to compute precisely; they are resolved by
    // the full re-parse triggered by the subsequent didChange.
    if !edits.is_empty() {
        let mut deltas: HashMap<usize, isize> = HashMap::new();
        for edit in &edits {
            // Only consider leading-whitespace edits (start at column 0).
            if edit.range.start.character == 0 {
                let line = edit.range.start.line;
                let old_len = edit.range.end.character as isize;
                let new_len = edit.new_text.encode_utf16().count() as isize;
                let delta = new_len - old_len;
                if delta != 0 {
                    *deltas.entry(line).or_insert(0) += delta;
                }
            }
        }
        if !deltas.is_empty() {
            if let Some(snap) = FILE_STORE.get(uri) {
                if let Ok(mut hub) = snap.value().semantic.write() {
                    hub.adjust_columns(&deltas);
                }
            }
        }
    }

    send(
        &ResponseMessage {
            jsonrpc: "2.0".into(),
            id,
            result: Some(&edits),
            error: None,
        },
    )
    .await;
}

