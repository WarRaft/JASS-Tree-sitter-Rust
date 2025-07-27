use std::sync::Arc;
use serde::Serialize;
use tokio::io;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

pub async fn send<T: Serialize>(writer: &Arc<Mutex<io::Stdout>>, message: &T) {
    let msg = serde_json::to_string(message).unwrap();
    let header = format!("Content-Length: {}\r\n\r\n", msg.len());

    let mut writer = writer.lock().await;
    writer.write_all(header.as_bytes()).await.unwrap();
    writer.write_all(msg.as_bytes()).await.unwrap();
    writer.flush().await.unwrap();
}
