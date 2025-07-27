use serde::Serialize;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::io::Stdout;
use tokio::sync::Mutex;

pub async fn send<T: Serialize>(writer: &Arc<Mutex<Stdout>>, message: &T) {
    let msg_bytes = serde_json::to_vec(message).expect("Failed to serialize LSP message");
    let header = format!("Content-Length: {}\r\n\r\n", msg_bytes.len());

    let mut writer = writer.lock().await;
    writer.write_all(header.as_bytes()).await.unwrap();
    writer.write_all(&msg_bytes).await.unwrap();
    writer.flush().await.unwrap();
}
