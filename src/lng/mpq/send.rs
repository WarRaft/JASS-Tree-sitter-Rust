use crate::lsp::cancel::CancelId;
use crate::lsp::protocol::ResponseMessage;
use crate::lsp::send::send as lsp_send;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use log::error;
use serde_json::json;
use std::sync::Arc;
use tokio::io::Stdout;
use tokio::sync::Mutex;

/// Handle `mpq/info` — return archive metadata for the custom editor page.
pub async fn send_info(
    writer: &Arc<Mutex<Stdout>>,
    call_id: Option<CancelId>,
    archive_path: &str,
) {
    let path = archive_path.to_string();
    let result = tokio::task::spawn_blocking(move || get_info(&path))
        .await
        .unwrap_or_else(|e| Err(format!("spawn_blocking: {}", e)));

    let result_json = match result {
        Ok(info) => info,
        Err(e) => {
            error!("mpq/info error: {}", e);
            json!({ "error": e })
        }
    };

    lsp_send(
        writer,
        &ResponseMessage {
            jsonrpc: "2.0".into(),
            id: call_id,
            result: Some(result_json),
            error: None,
        },
    )
    .await;
}

/// Handle `mpq/list` — return the flat list of files inside an MPQ archive.
pub async fn send_list(
    writer: &Arc<Mutex<Stdout>>,
    call_id: Option<CancelId>,
    archive_path: &str,
) {
    let path = archive_path.to_string();
    let result = tokio::task::spawn_blocking(move || list_files(&path))
        .await
        .unwrap_or_else(|e| Err(format!("spawn_blocking: {}", e)));

    let result_json = match result {
        Ok(entries) => json!({ "entries": entries }),
        Err(e) => {
            error!("mpq/list error: {}", e);
            json!({ "error": e })
        }
    };

    lsp_send(
        writer,
        &ResponseMessage {
            jsonrpc: "2.0".into(),
            id: call_id,
            result: Some(result_json),
            error: None,
        },
    )
    .await;
}

/// Handle `mpq/read` — read a single file from an MPQ archive, return base64.
pub async fn send_read(
    writer: &Arc<Mutex<Stdout>>,
    call_id: Option<CancelId>,
    archive_path: &str,
    file_path: &str,
) {
    let apath = archive_path.to_string();
    let fpath = file_path.to_string();
    let result = tokio::task::spawn_blocking(move || read_file(&apath, &fpath))
        .await
        .unwrap_or_else(|e| Err(format!("spawn_blocking: {}", e)));

    let result_json = match result {
        Ok(data) => {
            let encoded = BASE64.encode(&data);
            json!({ "content": encoded, "size": data.len() })
        }
        Err(e) => {
            error!("mpq/read error: {}", e);
            json!({ "error": e })
        }
    };

    lsp_send(
        writer,
        &ResponseMessage {
            jsonrpc: "2.0".into(),
            id: call_id,
            result: Some(result_json),
            error: None,
        },
    )
    .await;
}

/// Well-known filenames found in W3X / W3M map archives.
/// Many maps ship without a `(listfile)`, so we probe these explicitly.
const KNOWN_MPQ_FILES: &[&str] = &[
    // internal metadata
    "(listfile)",
    "(attributes)",
    "(signature)",
    // scripts
    "war3map.j",
    "Scripts\\war3map.j",
    "war3map.lua",
    "Scripts\\war3map.lua",
    // map data
    "war3map.w3e",
    "war3map.wts",
    "war3map.w3i",
    "war3map.wtg",
    "war3map.wct",
    "war3map.w3r",
    "war3map.w3s",
    "war3map.w3c",
    "war3map.doo",
    "war3mapUnits.doo",
    // object data
    "war3map.w3u",
    "war3map.w3t",
    "war3map.w3a",
    "war3map.w3b",
    "war3map.w3d",
    "war3map.w3h",
    "war3map.w3q",
    // skin / misc
    "war3mapSkin.txt",
    "war3mapMisc.txt",
    "war3mapExtra.txt",
    // minimap & preview
    "war3map.mmp",
    "war3map.shd",
    "war3mapMap.blp",
    "war3mapMap.b00",
    "war3mapMap.tga",
    "war3mapPath.tga",
    "war3mapPreview.tga",
    "war3mapPreview.blp",
    // imported resources
    "war3mapImported\\war3mapImported.txt",
    "war3mapImported/war3mapImported.txt",
];

fn list_files(archive_path: &str) -> Result<Vec<serde_json::Value>, String> {
    let archive =
        storm_rs::MpqArchive::open(archive_path).map_err(|e| format!("Cannot open archive: {}", e))?;

    // Start with whatever (listfile) provides.
    let mut names: std::collections::HashSet<String> =
        archive.list_files().into_iter().collect();

    // Probe well-known filenames that may be missing from (listfile).
    for &name in KNOWN_MPQ_FILES {
        if !names.contains(name) && archive.has_file(name) {
            names.insert(name.to_string());
        }
    }

    let mut entries: Vec<serde_json::Value> = names
        .iter()
        .map(|name| {
            let size = archive.get_file_size(name).unwrap_or(0);
            json!({ "name": name, "size": size })
        })
        .collect();

    // Sort for stable display order.
    entries.sort_by(|a, b| {
        let na = a["name"].as_str().unwrap_or("");
        let nb = b["name"].as_str().unwrap_or("");
        na.to_ascii_lowercase().cmp(&nb.to_ascii_lowercase())
    });

    Ok(entries)
}

fn read_file(archive_path: &str, file_path: &str) -> Result<Vec<u8>, String> {
    let archive =
        storm_rs::MpqArchive::open(archive_path).map_err(|e| format!("Cannot open archive: {}", e))?;

    archive
        .read_file(file_path)
        .map_err(|e| format!("Cannot read file '{}': {}", file_path, e))
}

/// Gather archive metadata for the custom editor page.
fn get_info(archive_path: &str) -> Result<serde_json::Value, String> {
    // ── 1. Parse W3X/W3M file header (before the MPQ data) ──
    let mut header = parse_w3x_header(archive_path);

    // ── 2. Open as MPQ archive ──────────────────────────────
    let archive = storm_rs::MpqArchive::open(archive_path)
        .map_err(|e| format!("Cannot open archive: {}", e))?;

    // ── 3. File list ────────────────────────────────────────
    let mut names: std::collections::HashSet<String> =
        archive.list_files().into_iter().collect();
    for &name in KNOWN_MPQ_FILES {
        if !names.contains(name) && archive.has_file(name) {
            names.insert(name.to_string());
        }
    }

    let file_count = names.len();
    let total_size: u64 = names.iter()
        .map(|n| archive.get_file_size(n).unwrap_or(0) as u64)
        .sum();

    let mut files: Vec<serde_json::Value> = names.iter().map(|name| {
        let size = archive.get_file_size(name).unwrap_or(0);
        json!({ "name": name, "size": size })
    }).collect();
    files.sort_by(|a, b| {
        let na = a["name"].as_str().unwrap_or("");
        let nb = b["name"].as_str().unwrap_or("");
        na.to_ascii_lowercase().cmp(&nb.to_ascii_lowercase())
    });

    // ── 4. Try to parse war3map.w3i for detailed map info ───
    let mut w3i = json!(null);
    if let Ok(w3i_data) = archive.read_file("war3map.w3i") {
        if let Ok((data, meta)) = crate::lng::w3i::W3iData::read(&w3i_data) {
            if let Ok(mut val) = serde_json::to_value(data) {
                val["_meta"] = serde_json::to_value(meta).unwrap_or(json!(null));
                w3i = val;
            }
        }
    }

    // ── 4b. Read war3map.wts and resolve TRIGSTR_ references ─
    let wts_map = archive
        .read_file("war3map.wts")
        .map(|data| crate::lng::wts::trigstr_resolve::parse_wts_strings(&data))
        .unwrap_or_default();

    if !wts_map.is_empty() {
        crate::lng::wts::trigstr_resolve::resolve_trigstr_json(&mut header, &wts_map);
        crate::lng::wts::trigstr_resolve::resolve_trigstr_json(&mut w3i, &wts_map);
    }

    // ── 5. Try to read minimap image ────────────────────────
    let mut minimap = json!(null);
    if let Ok(blp_data) = archive.read_file("war3mapMap.blp") {
        minimap = json!({ "format": "blp", "size": blp_data.len() });
        // Try to decode BLP → PNG data-URL for display
        if let Ok(mut img) = blp::core::image::ImageBlp::from_buf(&blp_data) {
            if img.decode(&blp_data, &[]).is_ok() {
                if let Some(mip) = img.mipmaps.first() {
                    if let Some(ref rgba) = mip.image {
                        let dynamic = image::DynamicImage::ImageRgba8(rgba.clone());
                        let mut cursor = std::io::Cursor::new(Vec::new());
                        if dynamic.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                            let png_bytes = cursor.into_inner();
                            let data_url = format!("data:image/png;base64,{}", BASE64.encode(&png_bytes));
                            minimap = json!({
                                "format": "blp",
                                "size": blp_data.len(),
                                "dataUrl": data_url,
                                "width": mip.width,
                                "height": mip.height,
                            });
                        }
                    }
                }
            }
        }
    }

    // ── 6. Try to read preview image (war3mapPreview.tga / .blp) ──
    let mut preview = json!(null);
    if let Ok(tga_data) = archive.read_file("war3mapPreview.tga") {
        if let Ok(dyn_img) = image::load_from_memory_with_format(&tga_data, image::ImageFormat::Tga) {
            let rgba = dyn_img.to_rgba8();
            let w = rgba.width();
            let h = rgba.height();
            let mut cursor = std::io::Cursor::new(Vec::new());
            if image::DynamicImage::ImageRgba8(rgba).write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                let png_bytes = cursor.into_inner();
                let data_url = format!("data:image/png;base64,{}", BASE64.encode(&png_bytes));
                preview = json!({
                    "format": "tga",
                    "size": tga_data.len(),
                    "dataUrl": data_url,
                    "width": w,
                    "height": h,
                });
            }
        }
    }
    // Fallback: war3mapPreview.blp
    if preview.is_null() {
        if let Ok(blp_data) = archive.read_file("war3mapPreview.blp") {
            if let Ok(mut img) = blp::core::image::ImageBlp::from_buf(&blp_data) {
                if img.decode(&blp_data, &[]).is_ok() {
                    if let Some(mip) = img.mipmaps.first() {
                        if let Some(ref rgba) = mip.image {
                            let dynamic = image::DynamicImage::ImageRgba8(rgba.clone());
                            let mut cursor = std::io::Cursor::new(Vec::new());
                            if dynamic.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                                let png_bytes = cursor.into_inner();
                                let data_url = format!("data:image/png;base64,{}", BASE64.encode(&png_bytes));
                                preview = json!({
                                    "format": "blp",
                                    "size": blp_data.len(),
                                    "dataUrl": data_url,
                                    "width": mip.width,
                                    "height": mip.height,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(json!({
        "header": header,
        "fileCount": file_count,
        "totalSize": total_size,
        "files": files,
        "w3i": w3i,
        "minimap": minimap,
        "preview": preview,
    }))
}

/// Parse the W3X/W3M/W3N file header that sits before the MPQ data.
///
/// W3X/W3M format (from <https://www.hiveworkshop.com/threads/322007/>):
///   offset 0x00: char[4]  — "HM3W" signature
///   offset 0x04: u32      — unknown / header size
///   offset 0x08: string   — map name (null-terminated)
///   next:        u32      — map flags
///   next:        u32      — max players
///
/// W3N campaign format:
///   offset 0x00: char[4]  — "HM3C" signature
///   offset 0x04: u32      — campaign version
///   offset 0x08: u32      — editor version
///   offset 0x0C: string   — campaign name (null-terminated)
///   next:        string   — campaign difficulty (null-terminated)
fn parse_w3x_header(path: &str) -> serde_json::Value {
    use std::fs::File;
    use std::io::Read;

    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return json!(null),
    };

    let mut buf = vec![0u8; 512];
    let n = match file.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return json!(null),
    };
    buf.truncate(n);

    if buf.len() < 8 {
        return json!(null);
    }

    let sig = &buf[0..4];

    // ── W3X / W3M map header ─────────────────────────────────
    if sig == b"HM3W" {
        // Read null-terminated map name starting at offset 8
        let mut map_name = String::new();
        let mut pos = 8;
        while pos < buf.len() && buf[pos] != 0 {
            map_name.push(buf[pos] as char);
            pos += 1;
        }
        pos += 1; // skip null terminator

        let map_flags = if pos + 4 <= buf.len() {
            Some(u32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]))
        } else {
            None
        };
        if map_flags.is_some() { pos += 4; }

        let max_players = if pos + 4 <= buf.len() {
            Some(u32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]))
        } else {
            None
        };

        return json!({
            "signature": "HM3W",
            "mapName": map_name,
            "mapFlags": map_flags,
            "maxPlayers": max_players,
        });
    }

    // ── W3N campaign header ──────────────────────────────────
    if sig == b"HM3C" {
        let campaign_version = if buf.len() >= 8 {
            Some(u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]))
        } else {
            None
        };

        let editor_version = if buf.len() >= 12 {
            Some(u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]))
        } else {
            None
        };

        let mut pos = 12;

        // Campaign name (null-terminated)
        let mut campaign_name = String::new();
        while pos < buf.len() && buf[pos] != 0 {
            campaign_name.push(buf[pos] as char);
            pos += 1;
        }
        pos += 1; // skip null terminator

        // Campaign difficulty (null-terminated)
        let mut campaign_difficulty = String::new();
        while pos < buf.len() && buf[pos] != 0 {
            campaign_difficulty.push(buf[pos] as char);
            pos += 1;
        }

        return json!({
            "signature": "HM3C",
            "campaignVersion": campaign_version,
            "editorVersion": editor_version,
            "campaignName": campaign_name,
            "campaignDifficulty": campaign_difficulty,
        });
    }

    // Not a recognized W3X/W3M/W3N header — might be a plain MPQ
    json!({ "signature": format!("{:02X}{:02X}{:02X}{:02X}", sig[0], sig[1], sig[2], sig[3]) })
}


