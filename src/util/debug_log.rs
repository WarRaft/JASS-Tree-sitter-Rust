//! WebSocket-based debug log broadcaster.
//!
//! Usage:
//!   `debug_log!("document_update: init done for {}", uri);`
//!
//! On the client side, connect to `ws://127.0.0.1:{port}/ws/log?token=...`
//! and every `debug_log!` message will arrive as a text frame.

use once_cell::sync::Lazy;
use tokio::sync::broadcast;

/// Broadcast channel for debug log messages.
/// 64 slots — if no client is connected the messages are silently dropped.
static TX: Lazy<broadcast::Sender<String>> = Lazy::new(|| broadcast::channel(64).0);

/// Send a debug message to all connected WebSocket clients.
pub fn send(msg: String) {
    let _ = TX.send(msg);
}

/// Subscribe to the debug log stream (one receiver per WS client).
pub fn subscribe() -> broadcast::Receiver<String> {
    TX.subscribe()
}

/// Convenience macro — works like `format!` but sends via the debug WS.
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        $crate::util::debug_log::send(format!($($arg)*))
    };
}

