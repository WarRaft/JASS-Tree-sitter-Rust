//! Wire protocol types — notifications only.
//!
//! All request/response methods are now served via HTTP routes
//! (see `http::api`). WebSocket carries only document-sync
//! notifications.

use crate::lsp::text_document::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams,
};
use serde::{Deserialize, Serialize};
use url::Url;

// ═══════════════════════════════════════════════════════════════════════════════
//  WebSocket notification — the only message type on the wire now
// ═══════════════════════════════════════════════════════════════════════════════

/// Inbound WebSocket message — only notifications (no id, no jsonrpc).
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum WsNotification {
    #[serde(rename = "document/close")]
    DidClose(DidCloseTextDocumentParams),

    #[serde(rename = "document/open")]
    DidOpen(DidOpenTextDocumentParams),

    #[serde(rename = "document/change")]
    DidChange(DidChangeTextDocumentParams),

    #[serde(rename = "files/changed")]
    DidChangeWatchedFiles(DidChangeWatchedFilesParams),
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Param structs used by HTTP route handlers (kept here to avoid circular deps)
// ═══════════════════════════════════════════════════════════════════════════════

/// Params for `slk/edit` — edit a single cell in the SLK table.
#[derive(Debug, Serialize, Deserialize)]
pub struct SlkEditParams {
    pub uri: Url,
    /// Byte offset of the old value in the document.
    pub start: usize,
    /// Byte length of the old value.
    pub len: usize,
    /// New cell value (raw text to insert).
    pub value: String,
}
