use crate::lsp::cancel::CancelId;
use once_cell::sync::OnceCell;
use serde::Serialize;
use tokio::sync::mpsc;

/// Global sender for outgoing messages (→ WebSocket).
static SEND_TX: OnceCell<mpsc::UnboundedSender<String>> = OnceCell::new();

/// Set the global outgoing message sender (called once from WS handler).
pub fn init_sender(tx: mpsc::UnboundedSender<String>) {
    let _ = SEND_TX.set(tx);
}

pub async fn send<T: Serialize>(message: &T) {
    let json = serde_json::to_string(message).expect("Failed to serialize message");
    if let Some(tx) = SEND_TX.get() {
        let _ = tx.send(json);
    }
}

/// Send a `RequestCancelled` error response (code −32800) for a cancelled request.
pub async fn send_cancelled(id: Option<CancelId>) {
    crate::util::debug_log::send_debug_log(
        "response",
        crate::util::debug_log::DebugStatus::Cancelled,
        &id,
        None,
        None,
    )
    .await;

    send(
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32800,
                "message": "Request cancelled"
            }
        }),
    )
    .await;
}
