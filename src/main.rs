pub(crate) mod http;
pub(crate) mod util;

pub(crate) mod lng;

use serde_json::json;

#[tokio::main]
async fn main() {
    env_logger::init();

    // ── Eagerly load persistent caches from redb ───────────────
    // These are Lazy statics that normally init on first access.
    // Force them now so that by the time we print the port (which
    // tells the extension to start sending document/update), all
    // import trees are ready.
    {
        use util::import_graph::IMPORT_GRAPH;

        // Touch the Lazy static to trigger load from redb.
        let _ = IMPORT_GRAPH.all_uris();
        log::info!("Persistent caches loaded");
    }

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
        let gp = lng::map_editor::game_path::get_game_path();
        if !gp.is_empty() {
            log::info!("Game path found on startup, building snapshot…");
            lng::map_editor::snapshot::build_snapshot(None);
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
