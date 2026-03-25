use crate::lng::slk::kind::Kind;
use crate::lsp::cancel::CancelId;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use crate::util::roper::node::NodeExt;
use crate::util::roper::uri_map::ROPE_MAP;
use crate::util::tree_map::TREE_MAP;
use serde::{Deserialize, Serialize};
use serde_json::{json, to_value};
use std::error::Error;
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;
use url::Url;

// ─── Response types ──────────────────────────────────────────────────────────

/// A single cell in the SLK grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlkCell {
    /// Display value (stripped of SYLK quoting).
    pub value: String,
    /// Byte offset of the K-field value node in the document (for editing).
    /// `None` for empty / implicit cells.
    pub start: Option<usize>,
    /// Byte length of the value node.
    pub len: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct SlkTableResponse {
    pub cols: usize,
    pub rows: usize,
    /// Column-major grid: `grid[row][col]`.
    pub grid: Vec<Vec<SlkCell>>,
}

// ─── LSP entry point ─────────────────────────────────────────────────────────

pub async fn send(writer: &Arc<Mutex<Stdout>>, call_id: Option<CancelId>, uri: &Url) {
    let result_json = _send(uri).unwrap_or_else(|e| {
        json!({
            "error": { "message": e.to_string() }
        })
    });

    let _ = lsp_send(
        writer,
        &ResponseMessage {
            jsonrpc: "2.0".into(),
            id: call_id,
            result: Some(result_json),
            error: None,
        },
    )
    .await;
}

// ─── Core logic ──────────────────────────────────────────────────────────────

fn _send(uri: &Url) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let rope = ROPE_MAP
        .get(uri)
        .map(|r| r.value().clone())
        .ok_or("no rope")?;
    let tree = TREE_MAP
        .get(uri)
        .map(|t| t.value().clone())
        .ok_or("no tree")?;

    let root = tree.root_node();

    // Pass 1: find dimensions from the B record (;X<cols>;Y<rows>)
    let mut total_cols: usize = 0;
    let mut total_rows: usize = 0;

    // Pass 2: collect cell data from C records
    struct RawCell {
        row: usize,  // 1-based
        col: usize,  // 1-based
        value: String,
        start: usize,
        len: usize,
    }
    let mut cells: Vec<RawCell> = Vec::new();

    for i in 0..root.child_count() as u32 {
        let record = match root.child(i) {
            Some(n) => n,
            None => continue,
        };

        let kind = match Kind::try_from(record.grammar_id()) {
            Ok(k) => k,
            Err(_) => continue,
        };

        match kind {
            Kind::BRecord => {
                // B;X<cols>;Y<rows>;D0
                for j in 0..record.child_count() as u32 {
                    let child = match record.child(j) {
                        Some(n) => n,
                        None => continue,
                    };
                    if Kind::try_from(child.grammar_id()) != Ok(Kind::Field) {
                        continue;
                    }
                    let (tag, val) = field_tag_value(&child, &rope);
                    match tag.as_str() {
                        "X" => total_cols = val.parse().unwrap_or(0),
                        "Y" => total_rows = val.parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }
            Kind::CRecord => {
                // C;X<col>;Y<row>;K<value>
                let mut cur_col: Option<usize> = None;
                let mut cur_row: Option<usize> = None;
                let mut k_value: Option<(String, usize, usize)> = None;

                for j in 0..record.child_count() as u32 {
                    let child = match record.child(j) {
                        Some(n) => n,
                        None => continue,
                    };
                    if Kind::try_from(child.grammar_id()) != Ok(Kind::Field) {
                        continue;
                    }
                    let (tag, val, val_start, val_len) = field_tag_value_span(&child, &rope);
                    match tag.as_str() {
                        "X" => cur_col = val.parse().ok(),
                        "Y" => cur_row = val.parse().ok(),
                        "K" => k_value = Some((val, val_start, val_len)),
                        _ => {}
                    }
                }

                if let Some((value, start, len)) = k_value {
                    // SYLK uses sticky row/col: if Y is absent, use previous row.
                    // We record whatever is present; the grid builder handles defaults.
                    cells.push(RawCell {
                        row: cur_row.unwrap_or(0),
                        col: cur_col.unwrap_or(0),
                        value: strip_slk_quotes(&value),
                        start,
                        len,
                    });
                }
            }
            _ => {}
        }
    }

    // Fallback dimensions: derive from cell coordinates if B record is missing.
    if total_cols == 0 || total_rows == 0 {
        for c in &cells {
            if c.col > total_cols {
                total_cols = c.col;
            }
            if c.row > total_rows {
                total_rows = c.row;
            }
        }
    }

    // Build grid (0-indexed internally; SLK uses 1-based coords).
    let empty_cell = SlkCell {
        value: String::new(),
        start: None,
        len: None,
    };
    let mut grid: Vec<Vec<SlkCell>> = (0..total_rows)
        .map(|_| (0..total_cols).map(|_| empty_cell.clone()).collect())
        .collect();

    // SYLK sticky state: track the last seen Y so that C records without Y
    // inherit the previous row (the spec says Y is sticky).
    // We need to re-walk the C records in order to resolve sticky Y values.
    let mut sticky_row: usize = 1;
    let mut sticky_col: usize = 1;

    // We need to re-iterate in document order to handle sticky values properly.
    // Rebuild from root again with sticky tracking.
    let mut cell_idx = 0;
    for i in 0..root.child_count() as u32 {
        let record = match root.child(i) {
            Some(n) => n,
            None => continue,
        };
        if Kind::try_from(record.grammar_id()) != Ok(Kind::CRecord) {
            continue;
        }

        let mut cur_col: Option<usize> = None;
        let mut cur_row: Option<usize> = None;
        let mut has_k = false;

        for j in 0..record.child_count() as u32 {
            let child = match record.child(j) {
                Some(n) => n,
                None => continue,
            };
            if Kind::try_from(child.grammar_id()) != Ok(Kind::Field) {
                continue;
            }
            let (tag, _val) = field_tag_value(&child, &rope);
            match tag.as_str() {
                "X" => cur_col = _val.parse().ok(),
                "Y" => cur_row = _val.parse().ok(),
                "K" => has_k = true,
                _ => {}
            }
        }

        if let Some(r) = cur_row {
            sticky_row = r;
        }
        if let Some(c) = cur_col {
            sticky_col = c;
        }

        if has_k && cell_idx < cells.len() {
            let r = sticky_row.saturating_sub(1);
            let c = sticky_col.saturating_sub(1);
            if r < total_rows && c < total_cols {
                grid[r][c] = SlkCell {
                    value: cells[cell_idx].value.clone(),
                    start: Some(cells[cell_idx].start),
                    len: Some(cells[cell_idx].len),
                };
            }
            cell_idx += 1;
        }
    }

    let resp = SlkTableResponse {
        cols: total_cols,
        rows: total_rows,
        grid,
    };

    Ok(to_value(resp)?)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extract (tag, value) from a `field` node.
fn field_tag_value(field: &tree_sitter::Node, rope: &lapce_xi_rope::Rope) -> (String, String) {
    let mut tag = String::new();
    let mut val = String::new();
    for k in 0..field.child_count() as u32 {
        let child = match field.child(k) {
            Some(n) => n,
            None => continue,
        };
        match Kind::try_from(child.grammar_id()) {
            Ok(Kind::FieldTag) => tag = child.text(rope).to_string(),
            Ok(Kind::FieldValue) => val = child.text(rope).to_string(),
            _ => {}
        }
    }
    (tag, val)
}

/// Like `field_tag_value` but also returns (byte_start, byte_len) of the value node.
fn field_tag_value_span(
    field: &tree_sitter::Node,
    rope: &lapce_xi_rope::Rope,
) -> (String, String, usize, usize) {
    let mut tag = String::new();
    let mut val = String::new();
    let mut start: usize = 0;
    let mut len: usize = 0;
    for k in 0..field.child_count() as u32 {
        let child = match field.child(k) {
            Some(n) => n,
            None => continue,
        };
        match Kind::try_from(child.grammar_id()) {
            Ok(Kind::FieldTag) => tag = child.text(rope).to_string(),
            Ok(Kind::FieldValue) => {
                val = child.text(rope).to_string();
                start = child.start_byte();
                len = child.end_byte() - child.start_byte();
            }
            _ => {}
        }
    }
    // If there is no value node, point at end of field for insertion.
    if len == 0 {
        start = field.end_byte();
    }
    (tag, val, start, len)
}

/// Strip SYLK-style quoting from a K-field value.
/// `"hello"` → `hello`, `TRUE` → `TRUE`, `42` → `42`.
fn strip_slk_quotes(s: &str) -> String {
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

