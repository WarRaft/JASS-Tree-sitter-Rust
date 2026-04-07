pub(crate) mod ass;
pub(crate) mod jass;

use crate::http::position::Position;
use crate::http::range::Range;
use serde::{Deserialize, Serialize};
use url::Url;

// ─── Request ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DocumentFormattingParams {
    pub uri: Url,
    pub options: FormattingOptions,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormattingOptions {
    pub tab_size: u32,
    pub insert_spaces: bool,
    #[serde(default)]
    pub trim_trailing_whitespace: Option<bool>,
    #[serde(default)]
    pub insert_final_newline: Option<bool>,
    #[serde(default)]
    pub trim_final_newlines: Option<bool>,
}

// ─── Response ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

impl TextEdit {
    /// Create an edit that replaces leading whitespace on a line.
    pub fn leading_ws(line: usize, old_ws_len: usize, new_ws: &str) -> Self {
        TextEdit {
            range: Range {
                start: Position { line, character: 0 },
                end: Position { line, character: old_ws_len },
            },
            new_text: new_ws.to_string(),
        }
    }

    /// Create an edit that replaces trailing whitespace on a line.
    pub fn trailing_ws(line: usize, trail_start: usize, trail_end: usize) -> Self {
        TextEdit {
            range: Range {
                start: Position { line, character: trail_start },
                end: Position { line, character: trail_end },
            },
            new_text: String::new(),
        }
    }
}

