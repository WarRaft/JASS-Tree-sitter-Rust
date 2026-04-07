//! Global sender for outgoing WebSocket notifications.
//!
//! Used only for server→client push (e.g. `custom/parseResult`,
//! `watchers/register`).  Request/response is now handled by HTTP.

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
