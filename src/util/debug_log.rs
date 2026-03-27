//! Debug log — pushes `custom/debugLog` notifications to the client.
//!
//! Each entry describes a lifecycle event of an LSP task (request or
//! notification) so the developer can see exactly what the server is
//! doing in real time.
//!
//! Logging is gated behind an [`AtomicBool`] flag that the client can
//! toggle via the `custom/debugLogEnable` notification (or by setting
//! `initializationOptions.debugLog = true`).

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// Whether debug logging is enabled.  Checked before every
/// notification to avoid unnecessary serialization & IO.
pub static DEBUG_LOG_ENABLED: AtomicBool = AtomicBool::new(false);

/// Stored initialize request (raw JSON Value) and response (serialized).
static INIT_REQUEST: OnceLock<serde_json::Value> = OnceLock::new();
static INIT_RESPONSE: OnceLock<serde_json::Value> = OnceLock::new();

/// Store the raw initialize request params.
pub fn store_init_request(val: serde_json::Value) {
    let _ = INIT_REQUEST.set(val);
}

/// Store the initialize response.
pub fn store_init_response(val: serde_json::Value) {
    let _ = INIT_RESPONSE.set(val);
}

/// Return stored init request + response as a JSON object.
pub fn get_init_data() -> serde_json::Value {
    serde_json::json!({
        "request": INIT_REQUEST.get().cloned().unwrap_or(serde_json::Value::Null),
        "response": INIT_RESPONSE.get().cloned().unwrap_or(serde_json::Value::Null),
    })
}

/// Lifecycle status of a task.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DebugStatus {
    /// Task received / created.
    Created,
    /// Task spawned and running.
    Running,
    /// Task was cancelled (via `$/cancelRequest`).
    Cancelled,
    /// Task finished and response sent.
    Completed,
}

/// A single debug log entry sent to the client.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugEntry {
    /// ISO-8601 timestamp with milliseconds.
    pub timestamp: String,
    /// LSP method name (e.g. `"textDocument/semanticTokens/full"`).
    pub method: String,
    /// Current lifecycle status.
    pub status: DebugStatus,
    /// Request ID (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    /// Optional extra detail (URI, error message, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Document URI associated with this request (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

/// Return the current UTC time formatted as ISO-8601 with milliseconds.
fn now_iso() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let millis = now.subsec_millis();

    // Manual UTC breakdown (no chrono dependency).
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    // Gregorian date from day count (epoch = 1970-01-01).
    let (year, month, day) = civil_from_days(days as i64);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, h, m, s, millis
    )
}

/// Convert a day count since Unix epoch to (year, month, day).
fn civil_from_days(mut z: i64) -> (i64, u32, u32) {
    z += 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Convert a `CancelId` to a `serde_json::Value` for the log entry.
fn cancel_id_to_value(id: &Option<crate::lsp::cancel::CancelId>) -> Option<serde_json::Value> {
    id.as_ref().map(|cid| serde_json::to_value(cid).unwrap_or_default())
}

/// Send a debug log entry to the client.
///
/// Does nothing if `DEBUG_LOG_ENABLED` is `false` or the global LSP writer
/// has not been initialised yet.
pub async fn send_debug_log(
    method: &str,
    status: DebugStatus,
    id: &Option<crate::lsp::cancel::CancelId>,
    detail: Option<String>,
    uri: Option<String>,
) {
    if !DEBUG_LOG_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let writer = match crate::util::file_store::LSP_WRITER.get() {
        Some(w) => w,
        None => return,
    };

    let entry = DebugEntry {
        timestamp: now_iso(),
        method: method.to_string(),
        status,
        id: cancel_id_to_value(id),
        detail,
        uri,
    };

    crate::lsp::send::send(
        writer,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "custom/debugLog",
            "params": entry
        }),
    )
    .await;
}


