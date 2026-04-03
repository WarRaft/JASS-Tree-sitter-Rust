//! Lightweight binary HTTP server for editor data.
//!
//! Runs alongside the LSP on `127.0.0.1:{random_port}`.
//! Webviews `fetch()` binary terrain/model data directly — zero JSON/base64 overhead.

use crate::http::file_lookup::file_lookup_handler;
use crate::http::game_path::{game_path_set_handler, game_path_status_handler};
use crate::http::mdx_texture::mdx_texture_handler;
use crate::http::path_tex::path_tex_handler;
use crate::http::slk::tile_textures_handler;
use crate::http::snapshot::snapshot_handler;
use crate::http::terrain::terrain_handler;
use axum::{Router, http::StatusCode, routing::{get, post}};
use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Global binary server state: port + session token.
pub static BINARY_SERVER: OnceCell<BinaryServerInfo> = OnceCell::new();

#[derive(Clone)]
pub struct BinaryServerInfo {
    #[allow(dead_code)]
    pub port: u16,
    pub token: String,
}

/// Auth query parameter — every request must include `?token=...`.
#[derive(Deserialize)]
pub struct TokenParam {
    pub token: String,
}

/// Validate the session token. Returns 403 on mismatch.
pub fn check_token(params: &TokenParam) -> Result<(), (StatusCode, &'static str)> {
    let info = BINARY_SERVER
        .get()
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "Server not ready"))?;
    if params.token != info.token {
        return Err((StatusCode::FORBIDDEN, "Invalid token"));
    }
    Ok(())
}

/// Start the binary HTTP server on `127.0.0.1:0` (OS-assigned port).
/// Returns the assigned port. The server runs in the background on the
/// current tokio runtime.
pub async fn start_server() -> std::io::Result<u16> {
    // Generate a session token from high-entropy inputs.
    // Not a CSPRNG, but unpredictable enough for a localhost-only guard:
    // SHA-256 of (nanosecond timestamp ‖ PID ‖ stack address via ASLR).
    let token = {
        use sha2::{Sha256, Digest};
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let pid = std::process::id();
        let stack_addr = &nanos as *const _ as usize;
        let mut h = Sha256::new();
        h.update(nanos.to_le_bytes());
        h.update(pid.to_le_bytes());
        h.update(stack_addr.to_le_bytes());
        h.finalize().iter().map(|b| format!("{:02x}", b)).collect::<String>()
    };

    let app = Router::new()
        .route("/w3e/terrain", get(terrain_handler))
        .route("/w3e/file", get(file_lookup_handler))
        .route("/w3e/snapshot", get(snapshot_handler))
        .route("/w3e/tileTextures", get(tile_textures_handler))
        .route("/w3e/gamePath/status", get(game_path_status_handler))
        .route("/w3e/gamePath/set", post(game_path_set_handler))
        .route("/w3e/pathTex", get(path_tex_handler))
        .route("/mdx/texture", get(mdx_texture_handler));

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr: SocketAddr = listener.local_addr()?;
    let port = addr.port();

    let _ = BINARY_SERVER.set(BinaryServerInfo {
        port,
        token: token.clone(),
    });

    log::info!("Binary HTTP server listening on http://127.0.0.1:{port}");

    // Spawn the server — it runs forever alongside the LSP loop.
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("Binary HTTP server error: {e}");
        }
    });

    Ok(port)
}

