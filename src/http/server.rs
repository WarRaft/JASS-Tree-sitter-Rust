//! Lightweight binary HTTP server for editor data.
//!
//! Runs alongside the LSP on `127.0.0.1:{random_port}`.
//! Webviews `fetch()` binary terrain/model data directly — zero JSON/base64 overhead.

use crate::http::blp_render::blp_render_handler;
use crate::http::file_lookup::file_lookup_handler;
use crate::http::game_path::{game_path_set_handler, game_path_status_handler};
use crate::http::mdx_texture::mdx_texture_handler;
use crate::http::path_tex::path_tex_handler;
use crate::http::tile_textures::tile_textures_handler;
use crate::http::snapshot::snapshot_handler;
use crate::http::terrain::terrain_handler;
use axum::{Router, http::StatusCode, routing::{get, post}, serve::{Listener, ListenerExt}};
use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower_http::cors::{CorsLayer, Any};

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
        // ── Existing routes ──────────────────────────────────────────
        .route("/w3e/terrain", get(terrain_handler))
        .route("/w3e/file", get(file_lookup_handler))
        .route("/w3e/snapshot", get(snapshot_handler))
        .route("/w3e/tileTextures", get(tile_textures_handler))
        .route("/w3e/gamePath/status", get(game_path_status_handler))
        .route("/w3e/gamePath/set", post(game_path_set_handler))
        .route("/w3e/pathTex", get(path_tex_handler))
        .route("/mdx/texture", get(mdx_texture_handler))
        .route("/blp/render", get(blp_render_handler))
        // ── Render routes (POST with JSON body) ──────────────────────
        .route("/render/blp", post(crate::http::api::blp_render))
        .route("/render/mdx", post(crate::http::api::mdx_render))
        .route("/render/doo", post(crate::http::api::doo_render))
        .route("/render/w3i", post(crate::http::api::w3i_render))
        .route("/render/w3e", post(crate::http::api::w3e_render))
        .route("/render/w3obj", post(crate::http::api::w3obj_render))
        .route("/render/slk", post(crate::http::api::slk_render))
        // ── SLK edit ─────────────────────────────────────────────────
        .route("/slk/edit", post(crate::http::api::slk_edit))
        // ── W3E catalog ──────────────────────────────────────────────
        .route("/w3e/catalog/terrain", post(crate::http::api::w3e_terrain_slk))
        .route("/w3e/catalog/doodads", post(crate::http::api::w3e_doodads_slk))
        .route("/w3e/catalog/units", post(crate::http::api::w3e_units_slk))
        .route("/w3e/catalog/destructables", post(crate::http::api::w3e_destructables_slk))
        .route("/w3e/lookupFile", post(crate::http::api::w3e_lookup_file))
        // ── MPQ ──────────────────────────────────────────────────────
        .route("/mpq/info", post(crate::http::api::mpq_info))
        .route("/mpq/list", post(crate::http::api::mpq_list))
        .route("/mpq/read", post(crate::http::api::mpq_read))
        // ── Graphs ───────────────────────────────────────────────────
        .route("/graph/import", post(crate::http::api::graph_import))
        .route("/graph/call", post(crate::http::api::graph_call))
        .route("/graph/type", post(crate::http::api::graph_type))
        // ── Build ────────────────────────────────────────────────────
        .route("/build/execute", post(crate::http::api::build_execute))
        .route("/build/hooks", post(crate::http::api::build_hooks))
        // ── Rescan & UJAPI ───────────────────────────────────────────
        .route("/rescan", post(crate::http::api::rescan_execute))
        .route("/ujapi/download", post(crate::http::api::ujapi_download))
        // ── Language features ────────────────────────────────────────
        .route("/lsp/completion", post(crate::http::api::completion))
        .route("/lsp/hover", post(crate::http::api::hover))
        .route("/lsp/highlight", post(crate::http::api::document_highlight))
        .route("/lsp/definition", post(crate::http::api::definition))
        .route("/lsp/references", post(crate::http::api::references))
        .route("/lsp/formatting", post(crate::http::api::formatting))
        .route("/lsp/prepareRename", post(crate::http::api::prepare_rename))
        .route("/lsp/rename", post(crate::http::api::rename))
        .route("/lsp/willRenameFiles", post(crate::http::api::will_rename_files))
        .route("/lsp/colorPresentation", post(crate::http::api::color_presentation))
        .route("/lsp/codeAction", post(crate::http::api::code_action))
        .route("/lsp/signatureHelp", post(crate::http::api::signature_help))
        .route("/lsp/callHierarchy/prepare", post(crate::http::api::call_hierarchy_prepare))
        .route("/lsp/callHierarchy/incoming", post(crate::http::api::call_hierarchy_incoming))
        .route("/lsp/callHierarchy/outgoing", post(crate::http::api::call_hierarchy_outgoing))
        .route("/lsp/typeHierarchy/prepare", post(crate::http::api::type_hierarchy_prepare))
        .route("/lsp/typeHierarchy/supertypes", post(crate::http::api::type_hierarchy_supertypes))
        .route("/lsp/typeHierarchy/subtypes", post(crate::http::api::type_hierarchy_subtypes))
        // ── Document sync (HTTP replacements for WS notifications) ──
        .route("/document/update", post(crate::http::api::document_update))
        .route("/document/close", post(crate::http::api::document_close))
        .route("/document/didRenameFiles", post(crate::http::api::did_rename_files))
        .route("/files/changed", post(crate::http::api::files_changed))
        // ── Binary data ─────────────────────────────────────────────
        .route("/semantic", get(crate::http::api::semantic_tokens))
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any));

    let listener = TcpListener::bind("127.0.0.1:0").await?
        .tap_io(|tcp_stream| {
            if let Err(e) = tcp_stream.set_nodelay(true) {
                log::error!("failed to set TCP_NODELAY: {e}");
            }
        });
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

