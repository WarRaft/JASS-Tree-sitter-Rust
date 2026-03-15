use crate::lsp::position::Position;
use crate::lsp::range::Range;
use crate::lsp::text_document::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#documentFormattingOptions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentFormattingOptions {}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#documentFormattingParams
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentFormattingParams {
    pub text_document: TextDocumentIdentifier,
    pub options: FormattingOptions,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#formattingOptions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormattingOptions {
    /// Size of a tab in spaces.
    pub tab_size: u32,
    /// Prefer spaces over tabs.
    pub insert_spaces: bool,
    /// Trim trailing whitespace on a line (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trim_trailing_whitespace: Option<bool>,
    /// Insert a final newline at end of file (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_final_newline: Option<bool>,
    /// Trim all newlines after the final newline (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trim_final_newlines: Option<bool>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textEdit
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

impl TextEdit {
    /// Create an edit that replaces leading whitespace on a line.
    ///
    /// `line` — 0-based line number.
    /// `old_ws_len` — number of characters of existing leading whitespace.
    /// `new_ws` — the replacement whitespace string.
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
    ///
    /// `line` — 0-based line number.
    /// `trail_start` — character offset where trailing whitespace begins.
    /// `trail_end` — character offset where the line ends (exclusive).
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

