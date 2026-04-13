use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use blp::ImageDecoder;
use serde_json::json;

/// The external listfile shipped with the extension — contains properly-cased
/// paths for well-known Warcraft III game files.
static LISTFILE_TXT: &str = include_str!("../../../listfile.txt");

/// Build a lookup map: lowercase normalised path → original (properly-cased) path.
fn build_listfile_map() -> std::collections::HashMap<String, String> {
    LISTFILE_TXT
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let trimmed = l.trim();
            let key = trimmed.replace('\\', "/").to_ascii_lowercase();
            (key, trimmed.to_string())
        })
        .collect()
}

use std::sync::LazyLock;

/// Global lookup: lowercase path → properly-cased path from the external listfile.
static LISTFILE_CASE_MAP: LazyLock<std::collections::HashMap<String, String>> =
    LazyLock::new(build_listfile_map);


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

/// Return the properly-cased version of `name` if found in the external listfile,
/// otherwise return the original name unchanged.
fn fix_case(name: &str) -> String {
    let key = name.replace('\\', "/").to_ascii_lowercase();
    LISTFILE_CASE_MAP
        .get(&key)
        .cloned()
        .unwrap_or_else(|| name.to_string())
}

/// Model-file modification rawcodes (field = "file" for models).
const MODEL_FIELD_IDS: &[&str] = &["dfil", "bfil", "umdl", "ifil"];
/// Variation-count modification rawcodes (field = "numVar").
const NUMVAR_FIELD_IDS: &[&str] = &["dvar", "bvar"];
/// Texture-path modification rawcodes.
const TEXTURE_FIELD_IDS: &[&str] = &["dptx", "bptx", "bptd", "bshd", "uico", "iico"];
/// Model extensions in priority order (.mdx before .mdl).
const MODEL_EXTS: &[&str] = &[".mdx", ".mdl"];
/// Texture extensions in priority order (.tga before .blp).
const TEXTURE_EXTS: &[&str] = &[".tga", ".blp"];

/// Heuristic: does this string look like a file path?
fn looks_like_path(s: &str) -> bool {
    if s.len() < 3 { return false; }
    if s.contains('\\') || s.contains('/') { return true; }
    if let Some(dot) = s.rfind('.') {
        let ext_len = s.len() - dot;
        if ext_len >= 2 && ext_len <= 5 { return true; }
    }
    false
}

/// Strip the file extension from a path (everything after the last `.`
/// that comes after the last path separator).  Returns the original
/// string unchanged when there is no extension.
fn strip_ext(path: &str) -> &str {
    let last_sep = path.rfind(['/', '\\']).unwrap_or(0);
    match path[last_sep..].rfind('.') {
        Some(i) => &path[..last_sep + i],
        None => path,
    }
}

/// Does the path have a recognised file extension?
fn has_ext(path: &str) -> bool {
    let last_sep = path.rfind(['/', '\\']).unwrap_or(0);
    path[last_sep..].rfind('.').map_or(false, |i| {
        let ext_len = path.len() - (last_sep + i);
        ext_len >= 2 && ext_len <= 5
    })
}

/// Push `base` with each of the given extensions into `out`.
fn push_with_exts(base: &str, exts: &[&str], out: &mut Vec<String>) {
    for ext in exts {
        out.push(format!("{base}{ext}"));
    }
}

/// Expand a single path into concrete probe candidates.
///
/// * Paths with `.mdx`/`.mdl` → also try the paired model extension.
/// * Paths with `.tga`/`.blp` → also try the paired texture extension.
/// * Extensionless paths + known field type → correct extension pair.
/// * Extensionless paths with unknown type → kept as-is (no blind expansion).
fn expand_path(path: &str, kind: PathKind, out: &mut Vec<String>) {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".mdx") || lower.ends_with(".mdl") {
        let base = strip_ext(path);
        push_with_exts(base, MODEL_EXTS, out);
    } else if lower.ends_with(".tga") || lower.ends_with(".blp") {
        let base = strip_ext(path);
        push_with_exts(base, TEXTURE_EXTS, out);
    } else if has_ext(path) {
        // Known extension that is neither model nor texture — keep as-is.
        out.push(path.to_string());
    } else {
        // No extension — use the field type to decide.
        match kind {
            PathKind::Model   => push_with_exts(path, MODEL_EXTS, out),
            PathKind::Texture => push_with_exts(path, TEXTURE_EXTS, out),
            PathKind::Unknown => out.push(path.to_string()),
        }
    }
}

/// Semantic type of a path extracted from object data.
#[derive(Clone, Copy)]
enum PathKind { Model, Texture, Unknown }

/// Extract model/texture paths from all object definitions, respecting
/// field semantics (model vs texture) and doodad/destructable variations.
fn extract_paths_from_object_data(
    data: &crate::lng::w3abdhqtu::parse::W3ObjectData,
    out: &mut Vec<String>,
) {
    use crate::lng::w3abdhqtu::parse::{ModificationValue, ObjectDefinition};

    fn from_defs(defs: &[ObjectDefinition], out: &mut Vec<String>) {
        for def in defs {
            // First pass: collect file path + numVar for this definition.
            let mut model_path: Option<String> = None;
            let mut num_var: u32 = 0;

            for set in &def.sets {
                for m in &set.modifications {
                    let mid = m.modification_id.text.as_str();

                    if NUMVAR_FIELD_IDS.contains(&mid) {
                        if let ModificationValue::Int(v) = &m.value {
                            num_var = *v as u32;
                        }
                    }

                    if let ModificationValue::Str(ref s) = m.value {
                        if !looks_like_path(s) { continue; }

                        let kind = if MODEL_FIELD_IDS.contains(&mid) {
                            PathKind::Model
                        } else if TEXTURE_FIELD_IDS.contains(&mid) {
                            PathKind::Texture
                        } else {
                            PathKind::Unknown
                        };

                        // Remember model path for variation expansion below.
                        if matches!(kind, PathKind::Model) {
                            model_path = Some(s.clone());
                        }

                        expand_path(s, kind, out);
                    }
                }
            }

            // Second pass: generate variation paths (base0, base1, …)
            // for model fields when numVar > 1.
            if num_var > 1 {
                if let Some(ref mp) = model_path {
                    let base = strip_ext(mp);
                    for i in 0..num_var {
                        let var_base = format!("{base}{i}");
                        push_with_exts(&var_base, MODEL_EXTS, out);
                    }
                }
            }
        }
    }

    from_defs(&data.table.originals, out);
    from_defs(&data.table.customs, out);
}

/// Collect candidate file paths by parsing the map's object data,
/// W3I, and imported-file lists.  Returns a deduplicated set of paths
/// to probe with `archive.has_file()`.
fn collect_candidate_paths(archive: &storm_rs::MpqArchive) -> std::collections::HashSet<String> {
    let mut raw: Vec<String> = Vec::new();

    // ── Object data files ──
    let obj_files: &[(&str, bool)] = &[
        ("war3map.w3a", true),   // abilities  (level-based)
        ("war3map.w3b", false),  // destructables
        ("war3map.w3d", true),   // doodads    (level-based)
        ("war3map.w3h", false),  // buffs
        ("war3map.w3q", true),   // upgrades   (level-based)
        ("war3map.w3t", false),  // items
        ("war3map.w3u", false),  // units
    ];
    for &(file_name, level_data) in obj_files {
        if let Ok(buf) = archive.read_file(file_name) {
            if let Ok((data, _)) = crate::lng::w3abdhqtu::parse::W3ObjectData::read(&buf, level_data) {
                extract_paths_from_object_data(&data, &mut raw);
            }
        }
    }

    // ── W3I paths (loading screen, prologue — these are models) ──
    if let Ok(buf) = archive.read_file("war3map.w3i") {
        if let Ok((w3i, _)) = crate::lng::w3i::W3iData::read(&buf) {
            if let Some(ref p) = w3i.loading_screen_model {
                if !p.is_empty() { expand_path(p, PathKind::Model, &mut raw); }
            }
            if let Some(ref p) = w3i.prologue_screen_model {
                if !p.is_empty() { expand_path(p, PathKind::Model, &mut raw); }
            }
        }
    }

    // ── Imported files list (paths already have extensions) ──
    for imp in &["war3mapImported\\war3mapImported.txt", "war3mapImported/war3mapImported.txt"] {
        if let Ok(buf) = archive.read_file(imp) {
            if let Ok(text) = String::from_utf8(buf) {
                for line in text.lines() {
                    let t = line.trim();
                    if !t.is_empty() {
                        raw.push(t.to_string());
                    }
                }
            }
            break;
        }
    }

    raw.into_iter().collect()
}

pub(crate) fn list_files_pub(archive_path: &str) -> Result<Vec<serde_json::Value>, String> {
    list_files(archive_path)
}

/// File source tag used while building the file list.
#[derive(Clone, Copy, PartialEq)]
enum FileSource {
    /// From the archive's internal `(listfile)`.
    Listfile,
    /// Probed from the `KNOWN_MPQ_FILES` constant.
    Discovered,
    /// Probed from paths referenced in the map's data files.
    Found,
}

fn list_files(archive_path: &str) -> Result<Vec<serde_json::Value>, String> {
    let archive =
        storm_rs::MpqArchive::open(archive_path).map_err(|e| format!("Cannot open archive: {}", e))?;

    // Start with whatever (listfile) provides.
    let raw_names: Vec<String> = archive.list_files().into_iter().collect();

    // Deduplicate by lowercase — MPQ paths are case-insensitive,
    // so two listfiles may provide the same path with different casing.
    // Prefer the properly-cased variant from the external listfile.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut names: Vec<(String, FileSource)> = Vec::new();
    for name in raw_names {
        let lower = name.replace('\\', "/").to_ascii_lowercase();
        if seen.insert(lower) {
            names.push((fix_case(&name), FileSource::Listfile));
        }
    }

    // Probe well-known filenames that may be missing from (listfile).
    for &name in KNOWN_MPQ_FILES {
        let lower = name.replace('\\', "/").to_ascii_lowercase();
        if !seen.contains(&lower) && archive.has_file(name) {
            seen.insert(lower);
            names.push((fix_case(name), FileSource::Discovered));
        }
    }

    // Probe paths referenced in the map's object data / w3i / imports.
    let candidates = collect_candidate_paths(&archive);
    for candidate in &candidates {
        let lower = candidate.replace('\\', "/").to_ascii_lowercase();
        if !seen.contains(&lower) && archive.has_file(candidate) {
            seen.insert(lower);
            names.push((fix_case(candidate), FileSource::Found));
        }
    }

    let mut entries: Vec<serde_json::Value> = names
        .iter()
        .map(|(name, source)| {
            let size = archive.get_file_size(name).unwrap_or(0);
            match source {
                FileSource::Discovered => json!({ "name": name, "size": size, "discovered": true }),
                FileSource::Found      => json!({ "name": name, "size": size, "found": true }),
                FileSource::Listfile   => json!({ "name": name, "size": size }),
            }
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

pub(crate) fn read_file_pub(archive_path: &str, file_path: &str) -> Result<Vec<u8>, String> {
    read_file(archive_path, file_path)
}

fn read_file(archive_path: &str, file_path: &str) -> Result<Vec<u8>, String> {
    let archive =
        storm_rs::MpqArchive::open(archive_path).map_err(|e| format!("Cannot open archive: {}", e))?;

    archive
        .read_file(file_path)
        .map_err(|e| format!("Cannot read file '{}': {}", file_path, e))
}

pub(crate) fn get_info_pub(archive_path: &str) -> Result<serde_json::Value, String> {
    get_info(archive_path)
}

/// Gather archive metadata for the custom editor page.
fn get_info(archive_path: &str) -> Result<serde_json::Value, String> {
    // ── 1. Parse W3X/W3M file header (before the MPQ data) ──
    let mut header = parse_w3x_header(archive_path);

    // ── 2. Open as MPQ archive ──────────────────────────────
    let archive = storm_rs::MpqArchive::open(archive_path)
        .map_err(|e| format!("Cannot open archive: {}", e))?;

    // ── 3. File list ────────────────────────────────────────
    let raw_names: Vec<String> = archive.list_files().into_iter().collect();

    // Deduplicate by lowercase — MPQ paths are case-insensitive.
    // Prefer properly-cased names from the external listfile.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut file_entries: Vec<(String, FileSource)> = Vec::new();
    for name in raw_names {
        let lower = name.replace('\\', "/").to_ascii_lowercase();
        if seen.insert(lower) {
            file_entries.push((fix_case(&name), FileSource::Listfile));
        }
    }
    for &name in KNOWN_MPQ_FILES {
        let lower = name.replace('\\', "/").to_ascii_lowercase();
        if !seen.contains(&lower) && archive.has_file(name) {
            seen.insert(lower);
            file_entries.push((fix_case(name), FileSource::Discovered));
        }
    }

    // Probe paths referenced in the map's object data / w3i / imports.
    let candidates = collect_candidate_paths(&archive);
    for candidate in &candidates {
        let lower = candidate.replace('\\', "/").to_ascii_lowercase();
        if !seen.contains(&lower) && archive.has_file(candidate) {
            seen.insert(lower);
            file_entries.push((fix_case(candidate), FileSource::Found));
        }
    }

    let file_count = file_entries.len();
    let total_size: u64 = file_entries.iter()
        .map(|(n, _)| archive.get_file_size(n).unwrap_or(0) as u64)
        .sum();

    let mut files: Vec<serde_json::Value> = file_entries.iter().map(|(name, source)| {
        let size = archive.get_file_size(name).unwrap_or(0);
        match source {
            FileSource::Discovered => json!({ "name": name, "size": size, "discovered": true }),
            FileSource::Found      => json!({ "name": name, "size": size, "found": true }),
            FileSource::Listfile   => json!({ "name": name, "size": size }),
        }
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
        if let Ok(img) = blp::Blp::into_dynamic(&blp_data) {
            let rgba = img.to_rgba8();
            let w = rgba.width();
            let h = rgba.height();
            let dynamic = image::DynamicImage::ImageRgba8(rgba);
            let mut cursor = std::io::Cursor::new(Vec::new());
            if dynamic.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                let png_bytes = cursor.into_inner();
                let data_url = format!("data:image/png;base64,{}", BASE64.encode(&png_bytes));
                minimap = json!({
                    "format": "blp",
                    "size": blp_data.len(),
                    "dataUrl": data_url,
                    "width": w,
                    "height": h,
                });
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
            if let Ok(img) = blp::Blp::into_dynamic(&blp_data) {
                let rgba = img.to_rgba8();
                let w = rgba.width();
                let h = rgba.height();
                let dynamic = image::DynamicImage::ImageRgba8(rgba);
                let mut cursor = std::io::Cursor::new(Vec::new());
                if dynamic.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                    let png_bytes = cursor.into_inner();
                    let data_url = format!("data:image/png;base64,{}", BASE64.encode(&png_bytes));
                    preview = json!({
                        "format": "blp",
                        "size": blp_data.len(),
                        "dataUrl": data_url,
                        "width": w,
                        "height": h,
                    });
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
        let mut pos = 8;
        let name_start = pos;
        while pos < buf.len() && buf[pos] != 0 {
            pos += 1;
        }
        let map_name = String::from_utf8_lossy(&buf[name_start..pos]).into_owned();
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
        let name_start = pos;
        while pos < buf.len() && buf[pos] != 0 {
            pos += 1;
        }
        let campaign_name = String::from_utf8_lossy(&buf[name_start..pos]).into_owned();
        pos += 1; // skip null terminator

        // Campaign difficulty (null-terminated)
        let diff_start = pos;
        while pos < buf.len() && buf[pos] != 0 {
            pos += 1;
        }
        let campaign_difficulty = String::from_utf8_lossy(&buf[diff_start..pos]).into_owned();

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

