use crate::lsp::cancel::CancelId;
use crate::lsp::inlay_hint::lsp::{InlayHint, InlayHintKind, InlayHintParams};
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use crate::util::file_store::FILE_STORE;
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;

/// Handle `textDocument/inlayHint`.
pub async fn send(
    writer: &Arc<Mutex<Stdout>>,
    id: Option<CancelId>,
    params: &InlayHintParams,
) {
    let uri = &params.text_document.uri;
    let result = compute(uri, Some(&params.range));

    lsp_send(
        writer,
        &ResponseMessage {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        },
    )
    .await;
}

/// Compute all inlay hints for a file (no range filter).
pub fn compute_all(uri: &url::Url) -> Vec<InlayHint> {
    compute(uri, None)
}

fn compute(
    uri: &url::Url,
    range: Option<&crate::lsp::range::Range>,
) -> Vec<InlayHint> {
    let snapshot = match FILE_STORE.get(uri) {
        Some(s) => s,
        None => return vec![],
    };

    let mut hints = Vec::new();

    // ── ujapi hints: always visible (version tag after path) ─────────────
    for hint in &snapshot.ujapi_hints {
        if in_range(&hint.position, range) {
            hints.push(hint.clone());
        }
    }

    let settings = &snapshot.file_symbols.file_settings;
    let ref_tip = settings.get("ref-tip").map(|v| v == "1").unwrap_or(false);
    let type_tip = settings.get("type-tip").map(|v| v == "1").unwrap_or(false);

    if !ref_tip && !type_tip {
        return hints;
    }

    // ── ref-tip: debug reference-ID hints ───────────────────────────────
    if ref_tip {
        let ref_map = &snapshot.ref_map;
        for span in &ref_map.spans {
            let pos = &span.range.start;
            if !in_range(pos, range) {
                continue;
            }

            let label = if span.is_external {
                ref_map
                    .external_decls
                    .get(&span.decl_key)
                    .map(|ext| {
                        let parts: Vec<String> = ext.origins.iter().map(|o| {
                            let path = o.uri.path();
                            let fname = path.rsplit('/').next().unwrap_or(path);
                            match o.origin_decl_key {
                                Some(ok) => format!("{}#{}", fname, ok),
                                None => fname.to_string(),
                            }
                        }).collect();
                        format!("\u{2192}{}", parts.join(","))
                    })
                    .unwrap_or_else(|| format!("#{}", span.decl_key))
            } else {
                format!("#{}", span.decl_key)
            };

            hints.push(InlayHint {
                position: span.range.end.clone(),
                label,
                kind: Some(InlayHintKind::Type),
                padding_left: Some(true),
                padding_right: Some(false),
                byte_offset: 0,
            });
        }
    }

    // ── type-tip: type-annotation hints ─────────────────────────────────
    if type_tip {
        for hint in &snapshot.type_hints {
            if in_range(&hint.position, range) {
                hints.push(hint.clone());
            }
        }
    }

    hints
}

/// Check whether a position falls inside the requested viewport range.
/// When `range` is `None`, the position is always considered in range (no filter).
fn in_range(
    pos: &crate::lsp::position::Position,
    range: Option<&crate::lsp::range::Range>,
) -> bool {
    let range = match range {
        Some(r) => r,
        None => return true,
    };
    if pos.line < range.start.line {
        return false;
    }
    if pos.line == range.start.line && pos.character < range.start.character {
        return false;
    }
    if pos.line > range.end.line {
        return false;
    }
    if pos.line == range.end.line && pos.character > range.end.character {
        return false;
    }
    true
}

