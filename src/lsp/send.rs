use serde::Serialize;
use serde_json::json;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::io::Stdout;
use tokio::sync::Mutex;

/// Counter for server-initiated request IDs (negative to avoid collision with client IDs).
static SERVER_REQUEST_ID: AtomicI64 = AtomicI64::new(-1);

pub async fn send<T: Serialize>(writer: &Arc<Mutex<Stdout>>, message: &T) {
    let msg_bytes = serde_json::to_vec(message).expect("Failed to serialize LSP message");
    let header = format!("Content-Length: {}\r\n\r\n", msg_bytes.len());

    let mut writer = writer.lock().await;
    writer.write_all(header.as_bytes()).await.unwrap();
    writer.write_all(&msg_bytes).await.unwrap();
    writer.flush().await.unwrap();
}

/// Send a server→client **request** (requires a unique `id`).
///
/// Use for `workspace/semanticTokens/refresh`, `workspace/inlayHint/refresh`,
/// `workspace/diagnostics/refresh`, etc.
pub async fn send_request(writer: &Arc<Mutex<Stdout>>, method: &str) {
    let id = SERVER_REQUEST_ID.fetch_sub(1, Ordering::Relaxed);
    send(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method
        }),
    )
    .await;
}

