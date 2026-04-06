//! Handlers for `color/presentation`.

use crate::lsp::cancel::CancelId;
use crate::lsp::color::lsp::*;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send;

pub async fn color_presentation_send(
    id: Option<CancelId>,
    params: &ColorPresentationParams,
) {
    // Determine whether this color sits inside a string literal.
    // Heuristic: if the range length is 10, it's likely a `|cAARRGGBB` pattern.
    let range_len = {
        let start_line = params.range.start.line;
        let end_line = params.range.end.line;
        if start_line == end_line {
            params.range.end.character.saturating_sub(params.range.start.character)
        } else {
            0
        }
    };

    let is_pipe_color = range_len == 10;

    let presentations = if is_pipe_color {
        // Inside a string: |cAARRGGBB
        let label = crate::lng::string_colors::color_to_pipe_string(&params.color);
        vec![ColorPresentation {
            label: label.clone(),
            text_edit: Some(TextEdit {
                range: params.range.clone(),
                new_text: label,
            }),
            additional_text_edits: None,
        }]
    } else {
        // Hex literal: 0xAARRGGBB
        let label = crate::lng::string_colors::color_to_hex_string(&params.color);
        vec![ColorPresentation {
            label: label.clone(),
            text_edit: Some(TextEdit {
                range: params.range.clone(),
                new_text: label,
            }),
            additional_text_edits: None,
        }]
    };

    send(
        &ResponseMessage {
            jsonrpc: "2.0".into(),
            id,
            result: Some(&presentations),
            error: None,
        },
    )
    .await;
}
