use crate::lng::w3e::parse::W3eData;
use crate::lng::w3e::slk::{load_terrain_slk, load_doodads_slk, load_units_slk, load_destructables_slk, load_cliff_types_slk};
use crate::lng::w3e::textures::load_tile_textures;
use crate::lsp::cancel::CancelId;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{json, to_value};
use std::error::Error;
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;
use url::Url;

// ── Pack terrain points into base64-encoded TypedArrays ─────────────────────
// Instead of serialising thousands of JSON objects with repeated keys,
// we pack each field into a flat binary array and base64-encode it.
// The webview decodes them into native TypedArrays — ~25× smaller and faster.

#[inline]
fn as_u8_slice_u16(data: &[u16]) -> &[u8] {
    // SAFETY: u16 has no padding; LE byte order matches JS Uint16Array.
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2) }
}

fn pack_points(data: &W3eData) -> serde_json::Value {
    let n = data.points.len();

    let mut ground_height: Vec<u16> = Vec::with_capacity(n);
    let mut ground_texture: Vec<u8> = Vec::with_capacity(n);
    let mut ground_variation: Vec<u8> = Vec::with_capacity(n);
    let mut cliff_variation: Vec<u8> = Vec::with_capacity(n);
    let mut cliff_texture: Vec<u8> = Vec::with_capacity(n);
    let mut layer_height: Vec<u8> = Vec::with_capacity(n);
    // Packed bit-flags per point: bit0=water, bit1=boundary, bit2=blight, bit3=ramp
    let mut flags: Vec<u8> = Vec::with_capacity(n);

    for p in &data.points {
        ground_height.push(p.ground_height);
        ground_texture.push(p.ground_texture);
        ground_variation.push(p.ground_variation);
        cliff_variation.push(p.cliff_variation);
        cliff_texture.push(p.cliff_texture);
        layer_height.push(p.layer_height);

        let mut f: u8 = 0;
        if p.water { f |= 1; }
        if p.boundary { f |= 2; }
        if p.blight { f |= 4; }
        if p.ramp { f |= 8; }
        flags.push(f);
    }

    json!({
        "groundHeight": BASE64.encode(as_u8_slice_u16(&ground_height)),
        "groundTexture": BASE64.encode(&ground_texture),
        "groundVariation": BASE64.encode(&ground_variation),
        "cliffVariation": BASE64.encode(&cliff_variation),
        "cliffTexture": BASE64.encode(&cliff_texture),
        "layerHeight": BASE64.encode(&layer_height),
        "flags": BASE64.encode(&flags),
    })
}

pub async fn send(
    writer: &Arc<Mutex<Stdout>>,
    call_id: Option<CancelId>,
    uri: &Url,
    archive_path: Option<&str>,
) {
    let result_json = _send(uri, archive_path).await.unwrap_or_else(|e| {
        json!({
            "error": {
                "message": e.to_string(),
                "kind": "w3e_render_failure"
            }
        })
    });

    let response = ResponseMessage {
        jsonrpc: "2.0".into(),
        id: call_id,
        result: Some(result_json),
        error: None,
    };

    let _ = lsp_send(writer, &response).await;
}

async fn _send(
    uri: &Url,
    archive_path: Option<&str>,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    if let Some(ap) = archive_path {
        // Opened from an MPQ archive — extract war3map.w3e
        let ap = ap.to_string();
        let file_name = uri
            .to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "war3map.w3e".into());

        // When the URI points to the archive itself (.w3x / .w3m / .w3n / .mpq),
        // the internal file we want is always "war3map.w3e".
        let file_name = {
            let lower = file_name.to_ascii_lowercase();
            if lower.ends_with(".w3x")
                || lower.ends_with(".w3m")
                || lower.ends_with(".w3n")
                || lower.ends_with(".mpq")
            {
                "war3map.w3e".to_string()
            } else {
                file_name
            }
        };

        let buf = tokio::task::spawn_blocking(move || {
            let archive = storm_rs::MpqArchive::open(&ap)
                .map_err(|e| format!("Cannot open archive: {e}"))?;
            archive
                .read_file(&file_name)
                .map_err(|e| format!("Cannot read {file_name}: {e}"))
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))??;

        let (data, meta) = W3eData::read(&buf)?;
        let ground_tiles = data.ground_tiles.clone();
        let packed = pack_points(&data);
        let mut val = to_value(data)?;
        val["_meta"] = to_value(meta)?;
        val["_packed"] = packed;

        // Attach terrain SLK tile metadata (blocking FS/MPQ reads).
        let ap2 = archive_path.map(|s| s.to_string());
        let slk_and_tex = tokio::task::spawn_blocking(move || {
            let slk = load_terrain_slk(ap2.as_deref());
            let tex = load_tile_textures(&ground_tiles, slk.as_ref(), ap2.as_deref());
            let dood_slk = load_doodads_slk(ap2.as_deref());
            let unit_slk = load_units_slk(ap2.as_deref());
            let dest_slk = load_destructables_slk(ap2.as_deref());
            let cliff_types_slk = load_cliff_types_slk(ap2.as_deref());
            (slk, tex, dood_slk, unit_slk, dest_slk, cliff_types_slk)
        })
        .await
        .ok();

        if let Some((slk, tex, dood_slk, unit_slk, dest_slk, cliff_types_slk)) = slk_and_tex {
            if let Some(slk_data) = slk {
                val["_terrainSlk"] = to_value(slk_data)?;
            }
            val["_tileTextures"] = to_value(tex)?;
            if let Some(dood_data) = dood_slk {
                val["_doodadsSlk"] = to_value(dood_data)?;
            }
            if let Some(unit_data) = unit_slk {
                val["_unitsSlk"] = to_value(unit_data)?;
            }
            if let Some(dest_data) = dest_slk {
                val["_destructablesSlk"] = to_value(dest_data)?;
            }
            if let Some(ct_data) = cliff_types_slk {
                val["_cliffTypesSlk"] = to_value(ct_data)?;
            }
        }

        Ok(val)
    } else {
        // Standalone file — read from disk.
        let path = uri.to_file_path().map_err(|()| "Invalid file URI")?;
        let buf = tokio::fs::read(&path).await?;
        let (data, meta) = W3eData::read(&buf)?;
        let ground_tiles = data.ground_tiles.clone();
        let packed = pack_points(&data);
        let mut val = to_value(data)?;
        val["_meta"] = to_value(meta)?;
        val["_packed"] = packed;

        // Attach terrain SLK tile metadata and textures.
        let slk_and_tex = tokio::task::spawn_blocking(move || {
            let slk = load_terrain_slk(None);
            let tex = load_tile_textures(&ground_tiles, slk.as_ref(), None);
            let dood_slk = load_doodads_slk(None);
            let unit_slk = load_units_slk(None);
            let dest_slk = load_destructables_slk(None);
            let cliff_types_slk = load_cliff_types_slk(None);
            (slk, tex, dood_slk, unit_slk, dest_slk, cliff_types_slk)
        })
        .await
        .ok();

        if let Some((slk, tex, dood_slk, unit_slk, dest_slk, cliff_types_slk)) = slk_and_tex {
            if let Some(slk_data) = slk {
                val["_terrainSlk"] = to_value(slk_data)?;
            }
            val["_tileTextures"] = to_value(tex)?;
            if let Some(dood_data) = dood_slk {
                val["_doodadsSlk"] = to_value(dood_data)?;
            }
            if let Some(unit_data) = unit_slk {
                val["_unitsSlk"] = to_value(unit_data)?;
            }
            if let Some(dest_data) = dest_slk {
                val["_destructablesSlk"] = to_value(dest_data)?;
            }
            if let Some(ct_data) = cliff_types_slk {
                val["_cliffTypesSlk"] = to_value(ct_data)?;
            }
        }

        Ok(val)
    }
}
