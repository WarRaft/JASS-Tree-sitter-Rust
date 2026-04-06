//! WebSocket handler — bidirectional channel between extension and server.
//!
//! Replaces the old stdin/stdout Content-Length framing.
//! - Incoming messages dispatched to the main message loop via `MSG_TX`.
//! - Outgoing messages sent via `SEND_TX` (set in `lsp::send`).

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::Query;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use once_cell::sync::OnceCell;
use tokio::sync::mpsc;

use crate::http::server::TokenParam;

/// Channel for incoming messages (WebSocket → main dispatch loop).
pub static MSG_TX: OnceCell<mpsc::UnboundedSender<String>> = OnceCell::new();

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<TokenParam>,
) -> impl IntoResponse {
    if let Err(e) = crate::http::server::check_token(&params) {
        return e.into_response();
    }
    ws.on_upgrade(handle_socket).into_response()
}

async fn handle_socket(socket: WebSocket) {
    log::info!("WebSocket client connected");

    let (mut sink, mut stream) = socket.split();

    // Create the outgoing channel and register it as the global sender.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    crate::lsp::send::init_sender(out_tx);

    // Task: forward outgoing messages from channel → WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if sink.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Read incoming messages and forward to the dispatch channel
    while let Some(result) = stream.next().await {
        match result {
            Ok(Message::Text(text)) => {
                if let Some(tx) = MSG_TX.get() {
                    let _ = tx.send(text.to_string());
                }
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                log::error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    // WebSocket disconnected — shut down
    send_task.abort();
    log::info!("WebSocket disconnected, shutting down");
    std::process::exit(0);
}

