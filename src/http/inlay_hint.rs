//! Inlay hint data types for the `custom/parseResult` notification.
//!
//! Sent as part of the custom protocol, not as a standard LSP response.
//! The encoding is intentionally minimal:
//!
//! - `position` is **flattened** (`line`, `character` at the top level)
//!   instead of nested `{"position": {"line": …, "character": …}}`.
//! - `paddingLeft` / `paddingRight` are **omitted** — all hints in this
//!   project use the same padding (`left = true`, `right = false`), so
//!   the JS side hardcodes them.
//! - `kind` is a plain `u8` (`0` = none, `1` = type, `2` = parameter)
//!   instead of `Option<enum>`.

use crate::http::position::Position;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// Inlay hint kind.
///
/// Wire values match VS Code's `InlayHintKind` enum:
/// `0` = unspecified, `1` = type annotation, `2` = parameter name.
#[derive(Debug, Serialize_repr, Deserialize_repr, Clone, Copy)]
#[repr(u8)]
pub enum InlayHintKind {
    /// No specific kind.
    None = 0,
    /// A type annotation hint (`: integer`).
    Type = 1,
    /// A parameter name hint (`x:`).
    Parameter = 2,
}

/// A single inlay hint displayed inline in the editor.
///
/// Wire format (JSON):
/// ```json
/// {"line": 5, "character": 12, "label": ": integer", "kind": 1}
/// ```
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct InlayHint {
    /// Position where the hint is rendered (flattened to `line` + `character`).
    #[serde(flatten)]
    pub position: Position,
    /// Text displayed as the hint label.
    pub label: String,
    /// Hint kind: `0` = none, `1` = type, `2` = parameter.
    pub kind: InlayHintKind,

    /// Byte offset of the AST node this hint is anchored to.
    ///
    /// Used internally by [`change::snap_to_tree_and_push`] to reposition
    /// hints instantly after incremental edits.  Not sent over the wire.
    #[serde(skip)]
    pub byte_offset: usize,
}

