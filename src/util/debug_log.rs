//! WebSocket-based debug log broadcaster.
//!
//! Usage:
//!   `debug_log!("document_update: init done for {}", uri);`
//!
//! On the client side, connect to `ws://127.0.0.1:{port}/ws/log?token=...`
//! and every `debug_log!` message will arrive as a text frame.

use once_cell::sync::Lazy;
use std::collections::VecDeque;
use std::sync::Mutex;
use tokio::sync::broadcast;

/// Broadcast channel for debug log messages.
/// 64 slots — if no client is connected the messages are silently dropped.
static TX: Lazy<broadcast::Sender<String>> = Lazy::new(|| broadcast::channel(64).0);
const RECENT_LIMIT: usize = 256;
static RECENT: Lazy<Mutex<VecDeque<String>>> = Lazy::new(|| Mutex::new(VecDeque::with_capacity(RECENT_LIMIT)));

/// Send a debug message to all connected WebSocket clients.
pub fn send(msg: String) {
    if let Ok(mut recent) = RECENT.lock() {
        if recent.len() >= RECENT_LIMIT {
            recent.pop_front();
        }
        recent.push_back(msg.clone());
    }
    let _ = TX.send(msg);
}

/// Subscribe to the debug log stream (one receiver per WS client).
pub fn subscribe() -> broadcast::Receiver<String> {
    TX.subscribe()
}

/// Return a snapshot of the recent debug log backlog.
pub fn recent() -> Vec<String> {
    RECENT
        .lock()
        .map(|recent| recent.iter().cloned().collect())
        .unwrap_or_default()
}

/// Convenience macro — works like `format!` but sends via the debug WS stream.
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        $crate::util::debug_log::send(format!($($arg)*))
    };
}
