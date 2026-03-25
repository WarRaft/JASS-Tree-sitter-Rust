use crate::lsp::protocol::SlkEditParams;
use crate::lsp::position::Position;
use crate::util::roper::uri_map::ROPE_MAP;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SlkEditResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// LSP Range of the old text — the client uses this to apply a TextEdit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<EditRange>,
    /// The new text to insert.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_text: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EditRange {
    pub start: EditPosition,
    pub end: EditPosition,
}

#[derive(Debug, Serialize)]
pub struct EditPosition {
    pub line: usize,
    pub character: usize,
}

/// Compute a text edit that replaces the old cell value with `params.value`.
///
/// Returns a range + newText that the extension applies via
/// `vscode.workspace.applyEdit`, giving free undo/redo.
pub fn apply_cell_edit(params: &SlkEditParams) -> SlkEditResult {
    let rope = match ROPE_MAP.get(&params.uri) {
        Some(r) => r.value().clone(),
        None => {
            return SlkEditResult {
                ok: false,
                message: Some("document not open".into()),
                range: None,
                new_text: None,
            };
        }
    };

    let start = match Position::from_byte_offset(&rope, params.start) {
        Some(p) => p,
        None => {
            return SlkEditResult {
                ok: false,
                message: Some("invalid start offset".into()),
                range: None,
                new_text: None,
            };
        }
    };

    let end = match Position::from_byte_offset(&rope, params.start + params.len) {
        Some(p) => p,
        None => {
            return SlkEditResult {
                ok: false,
                message: Some("invalid end offset".into()),
                range: None,
                new_text: None,
            };
        }
    };

    // Build the replacement value.  If the value looks like it needs SYLK
    // quoting (contains semicolons, newlines, or starts with a quote),
    // wrap it in quotes.  Numbers and booleans are left bare.
    let new_text = slk_encode_value(&params.value);

    SlkEditResult {
        ok: true,
        message: None,
        range: Some(EditRange {
            start: EditPosition {
                line: start.line,
                character: start.character,
            },
            end: EditPosition {
                line: end.line,
                character: end.character,
            },
        }),
        new_text: Some(new_text),
    }
}

/// Encode a user-provided value into SYLK K-field format.
///
/// - Pure numbers → bare  (`42`, `3.14`)
/// - `TRUE` / `FALSE` → bare
/// - Everything else → `"quoted"`
fn slk_encode_value(s: &str) -> String {
    // Empty → empty quoted string
    if s.is_empty() {
        return "\"\"".to_string();
    }

    // Boolean
    let upper = s.to_uppercase();
    if upper == "TRUE" || upper == "FALSE" {
        return upper;
    }

    // Number (integer or float)
    if s.parse::<f64>().is_ok() {
        return s.to_string();
    }

    // String — wrap in double quotes
    format!("\"{}\"", s)
}

