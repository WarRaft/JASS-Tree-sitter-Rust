use crate::lng::jass::symbol::FILE_SYMBOLS;
use crate::lsp::cancel::CancelId;
use crate::lsp::inlay_hint::lsp::{InlayHint, InlayHintKind, InlayHintParams};
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::ref_map::REF_URI_MAP;
use crate::lsp::send::send as lsp_send;
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
    let result = compute(uri, &params.range);

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

fn compute(
    uri: &url::Url,
    range: &crate::lsp::range::Range,
) -> Vec<InlayHint> {
    // Only show hints when `//set ref-tip 1` is present in the file header.
    let enabled = FILE_SYMBOLS
        .get(uri)
        .and_then(|e| e.value().file_settings.get("ref-tip").cloned())
        .map(|v| v == "1")
        .unwrap_or(false);
    if !enabled {
        return vec![];
    }

    let ref_entry = match REF_URI_MAP.get(uri) {
        Some(e) => e,
        None => return vec![],
    };
    let ref_map = ref_entry.value();

    let mut hints = Vec::new();

    for span in &ref_map.spans {
        let pos = &span.range.start;
        // Filter to requested range
        if pos.line < range.start.line
            || (pos.line == range.start.line && pos.character < range.start.character)
        {
            continue;
        }
        if pos.line > range.end.line
            || (pos.line == range.end.line && pos.character > range.end.character)
        {
            continue;
        }

        let label = if span.is_external {
            // Show origin filename + DeclKey for imported symbols
            ref_map
                .external_decls
                .get(&span.decl_key)
                .map(|ext| {
                    let path = ext.uri.path();
                    let fname = path.rsplit('/').next().unwrap_or(path);
                    match ext.origin_decl_key {
                        Some(ok) => format!("\u{2192}{}#{}", fname, ok), // →filename.j#53
                        None => format!("\u{2192}{}", fname),            // →filename.j
                    }
                })
                .unwrap_or_else(|| format!("#{}", span.decl_key))
        } else {
            format!("#{}", span.decl_key)
        };

        hints.push(InlayHint {
            // Place hint right after the identifier
            position: span.range.end.clone(),
            label,
            kind: Some(InlayHintKind::Type),
            padding_left: Some(true),
            padding_right: Some(false),
        });
    }

    hints
}

