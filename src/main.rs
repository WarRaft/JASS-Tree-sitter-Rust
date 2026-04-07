pub(crate) mod http;
pub(crate) mod util;

pub(crate) mod lng;

use serde_json::json;

#[tokio::main]
async fn main() {
    env_logger::init();

    // ── Start HTTP server ────────────────────────────────────────
    let http_port = http::server::start_server().await.ok();

    // ── Print port + token to stdout so the extension can connect ─
    if let (Some(port), Some(info)) = (http_port, http::server::BINARY_SERVER.get()) {
        let startup = json!({
            "port": port,
            "token": &info.token,
        });
        println!("{}", startup);
        use std::io::Write;
        std::io::stdout().flush().ok();
    }

    // ── Eagerly build snapshot if game path already configured ──
    tokio::task::spawn_blocking(|| {
        let gp = lng::w3e::game_path::get_game_path();
        if !gp.is_empty() {
            log::info!("Game path found on startup, building snapshot…");
            lng::w3e::snapshot::build_snapshot(None);
        }
    });

    // ── Stdin watcher: when extension dies stdin closes → we exit ─
    tokio::spawn(async {
        use tokio::io::AsyncReadExt;
        let mut stdin = tokio::io::stdin();
        let mut buf = [0u8; 64];
        loop {
            match stdin.read(&mut buf).await {
                Ok(0) | Err(_) => {
                    log::info!("stdin closed, shutting down");
                    std::process::exit(0);
                }
                Ok(_) => {}
            }
        }
    });

    // ── Park forever — all work happens in HTTP handlers ─────────
    std::future::pending::<()>().await;
}
