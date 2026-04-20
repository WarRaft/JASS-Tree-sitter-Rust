//! Cascading file lookup for Warcraft III data files.
//!
//! Search order:
//! 1a. The map archive itself (if `archive_path` is provided)
//! 1b. `{tileset}.mpq` inside the map archive (extracted to temp)
//! 2.  `{tileset}.mpq` from the game (pre-discovered: loose file or extracted from War3*.mpq)
//! 3.  The game folder directly (file on disk)
//! 4.  `War3Patch.mpq`
//! 5.  `War3xLocal.mpq`
//! 6.  `War3x.mpq`
//! 7.  `War3.mpq`

use crate::lng::map_editor::game_path::get_game_path;
use log::debug;
use serde::Serialize;
use std::path::Path;

/// MPQ archives to search (after the tileset MPQ).
const MPQ_SEARCH_ORDER: &[&str] = &[
    "War3Patch.mpq",
    "War3xLocal.mpq",
    "War3x.mpq",
    "War3.mpq",
];

/// Try to find `relative_path` (e.g. `"TerrainArt\Terrain.slk"`) using the
/// cascading lookup.  Returns `(file_bytes, source_label)` on success.
/// Automatically includes the global tileset MPQ (set via `set_tileset`).
pub fn lookup_file(relative_path: &str, archive_path: Option<&str>) -> Option<(Vec<u8>, String)> {
    let ts = super::game_path::get_tileset();
    lookup_file_ext(relative_path, archive_path, ts.as_deref())
}

/// Like `lookup_file`, but with an explicit tileset override.
pub fn lookup_file_ext(relative_path: &str, archive_path: Option<&str>, tileset: Option<&str>) -> Option<(Vec<u8>, String)> {
    lookup_file_resolved_ext(relative_path, archive_path, tileset)
        .map(|(buf, source, _resolved)| (buf, source))
}

/// Like `lookup_file`, but also returns the actual path that matched
/// (may differ from `relative_path` when `.mdx` → `.mdl` fallback triggers).
/// Automatically includes the global tileset MPQ.
pub fn lookup_file_resolved(relative_path: &str, archive_path: Option<&str>) -> Option<(Vec<u8>, String, String)> {
    let ts = super::game_path::get_tileset();
    lookup_file_resolved_ext(relative_path, archive_path, ts.as_deref())
}

/// Like `lookup_file_resolved`, but with an optional tileset for tileset-specific MPQ lookup.
pub fn lookup_file_resolved_ext(relative_path: &str, archive_path: Option<&str>, tileset: Option<&str>) -> Option<(Vec<u8>, String, String)> {
    // Try the exact path first (with extension fallbacks).
    if let result @ Some(_) = lookup_with_ext_fallback(relative_path, archive_path, tileset) {
        return result;
    }

    // Variation fallback: strip trailing digits from the filename stem and retry.
    // e.g. "Doodads\grass1" → "Doodads\grass"
    // e.g. "Doodads\grass1.mdx" → "Doodads\grass.mdx"
    if let Some(base) = strip_variation_digits(relative_path) {
        debug!("lookup_file: variation fallback {relative_path} → {base}");
        return lookup_with_ext_fallback(&base, archive_path, tileset);
    }

    None
}

/// Kind-aware resolved lookup.
///
/// `kind`:
/// - `Some("model")`   => only model extension fallback (.mdx/.mdl)
/// - `Some("texture")` => only texture extension fallback (.tga/.blp)
/// - otherwise          => generic lookup (`lookup_file_resolved_ext`)
pub fn lookup_file_resolved_kind_ext(
    relative_path: &str,
    archive_path: Option<&str>,
    tileset: Option<&str>,
    kind: Option<&str>,
) -> Option<(Vec<u8>, String, String)> {
    let kind_lc = kind.unwrap_or("").to_ascii_lowercase();
    if kind_lc == "model" {
        if let result @ Some(_) = lookup_model_with_ext_fallback(relative_path, archive_path, tileset) {
            return result;
        }
        if let Some(base) = strip_variation_digits(relative_path) {
            debug!("lookup_file(model): variation fallback {relative_path} -> {base}");
            return lookup_model_with_ext_fallback(&base, archive_path, tileset);
        }
        return None;
    }
    if kind_lc == "texture" {
        if let result @ Some(_) = lookup_texture_with_ext_fallback(relative_path, archive_path, tileset) {
            return result;
        }
        return None;
    }
    lookup_file_resolved_ext(relative_path, archive_path, tileset)
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelVariantFound {
    pub path: String,
    pub resolved_path: String,
    pub source: String,
}

fn no_ext_lower(path: &str) -> String {
    let p = path.replace('\\', "/").to_ascii_lowercase();
    let slash = p.rfind('/').unwrap_or(0);
    let dot = p.rfind('.');
    if let Some(d) = dot {
        if d > slash {
            return p[..d].to_string();
        }
    }
    p
}

fn model_variant_stem(search_path: &str) -> String {
    let last_sep = search_path.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    let filename = &search_path[last_sep..];
    let (stem, had_ext) = match filename.rfind('.') {
        Some(dot) => (&filename[..dot], true),
        None => (filename, false),
    };
    let base_stem = if had_ext {
        stem.trim_end_matches(|c: char| c.is_ascii_digit())
    } else {
        stem
    };
    format!("{}{}", &search_path[..last_sep], base_stem)
}

/// Resolve existing model variants for doodad/destructable model path.
///
/// Probe order (limit 10): `stem0..stem9`, where `stem` is derived from `search_path`:
/// - with extension: strip extension and trailing digits from filename stem
/// - without extension: keep filename stem as-is
///
/// If at least one digit variant is found, returns only found digit variants.
/// Otherwise probes base `stem` and returns it if found.
pub fn resolve_model_variants_ext(
    search_path: &str,
    archive_path: Option<&str>,
    tileset: Option<&str>,
) -> Vec<ModelVariantFound> {
    let stem = model_variant_stem(search_path);
    if stem.is_empty() {
        return Vec::new();
    }

    let mut digit_variants: Vec<ModelVariantFound> = Vec::new();
    for i in 0..10 {
        let candidate = format!("{}{}", stem, i);
        if let Some((_buf, source, resolved_path)) = lookup_model_with_ext_fallback(&candidate, archive_path, tileset) {
            // Strict path match by stem: candidate4 must not resolve to candidate.
            if no_ext_lower(&resolved_path) == no_ext_lower(&candidate) {
                digit_variants.push(ModelVariantFound {
                    path: candidate,
                    resolved_path,
                    source,
                });
            }
        }
    }
    if !digit_variants.is_empty() {
        return digit_variants;
    }

    if let Some((_buf, source, resolved_path)) = lookup_model_with_ext_fallback(&stem, archive_path, tileset) {
        if no_ext_lower(&resolved_path) == no_ext_lower(&stem) {
            return vec![ModelVariantFound {
                path: stem,
                resolved_path,
                source,
            }];
        }
    }

    Vec::new()
}

/// Core lookup with extension fallback (.mdx↔.mdl, .tga↔.blp) but
/// **without** variation digit stripping.
fn lookup_with_ext_fallback(relative_path: &str, archive_path: Option<&str>, tileset: Option<&str>) -> Option<(Vec<u8>, String, String)> {
    let lower = relative_path.to_ascii_lowercase();

    // ── Model extension normalization: always try .mdx first, then .mdl ──
    if lower.ends_with(".mdx") || lower.ends_with(".mdl") {
        let base = &relative_path[..relative_path.len() - 4];
        let mdx_path = format!("{base}.mdx");
        if let result @ Some(_) = lookup_cascade(&mdx_path, archive_path, tileset) {
            return result;
        }
        let mdl_path = format!("{base}.mdl");
        return lookup_cascade(&mdl_path, archive_path, tileset);
    }

    // ── Texture extension normalization: always try .tga first, then .blp ──
    if lower.ends_with(".tga") || lower.ends_with(".blp") {
        let base = &relative_path[..relative_path.len() - 4];
        let tga_path = format!("{base}.tga");
        if let result @ Some(_) = lookup_cascade(&tga_path, archive_path, tileset) {
            return result;
        }
        let blp_path = format!("{base}.blp");
        return lookup_cascade(&blp_path, archive_path, tileset);
    }

    // ── No extension: try as model (.mdx, .mdl), then as texture (.tga, .blp), then exact ──
    let last_sep = lower.rfind(['/', '\\']).unwrap_or(0);
    if !lower[last_sep..].contains('.') {
        // Model
        if let result @ Some(_) = lookup_cascade(&format!("{relative_path}.mdx"), archive_path, tileset) {
            return result;
        }
        if let result @ Some(_) = lookup_cascade(&format!("{relative_path}.mdl"), archive_path, tileset) {
            return result;
        }
        // Texture
        if let result @ Some(_) = lookup_cascade(&format!("{relative_path}.tga"), archive_path, tileset) {
            return result;
        }
        if let result @ Some(_) = lookup_cascade(&format!("{relative_path}.blp"), archive_path, tileset) {
            return result;
        }
    }

    lookup_cascade(relative_path, archive_path, tileset)
}

/// Model-only lookup with extension fallback (.mdx/.mdl) and no texture probing.
fn lookup_model_with_ext_fallback(relative_path: &str, archive_path: Option<&str>, tileset: Option<&str>) -> Option<(Vec<u8>, String, String)> {
    let lower = relative_path.to_ascii_lowercase();

    if lower.ends_with(".mdx") || lower.ends_with(".mdl") {
        let base = &relative_path[..relative_path.len() - 4];
        let mdx_path = format!("{base}.mdx");
        if let result @ Some(_) = lookup_cascade(&mdx_path, archive_path, tileset) {
            return result;
        }
        let mdl_path = format!("{base}.mdl");
        return lookup_cascade(&mdl_path, archive_path, tileset);
    }

    let last_sep = lower.rfind(['/', '\\']).unwrap_or(0);
    if !lower[last_sep..].contains('.') {
        if let result @ Some(_) = lookup_cascade(&format!("{relative_path}.mdx"), archive_path, tileset) {
            return result;
        }
        if let result @ Some(_) = lookup_cascade(&format!("{relative_path}.mdl"), archive_path, tileset) {
            return result;
        }
    }

    lookup_cascade(relative_path, archive_path, tileset)
}

/// Texture-only lookup with extension fallback (.tga/.blp) and no model probing.
fn lookup_texture_with_ext_fallback(relative_path: &str, archive_path: Option<&str>, tileset: Option<&str>) -> Option<(Vec<u8>, String, String)> {
    let lower = relative_path.to_ascii_lowercase();

    if lower.ends_with(".tga") || lower.ends_with(".blp") {
        let base = &relative_path[..relative_path.len() - 4];
        let tga_path = format!("{base}.tga");
        if let result @ Some(_) = lookup_cascade(&tga_path, archive_path, tileset) {
            return result;
        }
        let blp_path = format!("{base}.blp");
        return lookup_cascade(&blp_path, archive_path, tileset);
    }

    let last_sep = lower.rfind(['/', '\\']).unwrap_or(0);
    if !lower[last_sep..].contains('.') {
        if let result @ Some(_) = lookup_cascade(&format!("{relative_path}.tga"), archive_path, tileset) {
            return result;
        }
        if let result @ Some(_) = lookup_cascade(&format!("{relative_path}.blp"), archive_path, tileset) {
            return result;
        }
    }

    lookup_cascade(relative_path, archive_path, tileset)
}

/// Internal: search the full cascade for exactly `relative_path` (no extension fallbacks).
fn lookup_cascade(relative_path: &str, archive_path: Option<&str>, tileset: Option<&str>) -> Option<(Vec<u8>, String, String)> {
    // Normalise to backslash for MPQ lookups and forward-slash for FS.
    let mpq_path = relative_path.replace('/', "\\");
    let fs_path = relative_path.replace('\\', "/");

    // ── 1. Map archive ───────────────────────────────────────────
    if let Some(ap) = archive_path {
        if let Ok(archive) = storm_rs::MpqArchive::open(ap) {
            // 1a. Direct file in map archive.
            if let Ok(buf) = archive.read_file(&mpq_path) {
                debug!("lookup_file: FOUND {relative_path} in map archive");
                let label = format!("{{MAP}}\\{}", mpq_path);
                return Some((buf, label, relative_path.into()));
            }

            // 1b. Tileset MPQ inside map archive — extract, open, search.
            if let Some(ts) = tileset {
                if !ts.is_empty() {
                    let ch = ts.chars().next().unwrap_or('_').to_ascii_uppercase();
                    let ts_mpq = format!("{ch}.mpq");
                    if let Ok(ts_buf) = archive.read_file(&ts_mpq) {
                        let temp_dir = std::env::temp_dir().join("jass-tree-sitter").join("map-tileset");
                        if std::fs::create_dir_all(&temp_dir).is_ok() {
                            let temp_path = temp_dir.join(&ts_mpq);
                            if std::fs::write(&temp_path, &ts_buf).is_ok() {
                                if let Ok(ts_archive) = storm_rs::MpqArchive::open(temp_path.to_string_lossy().as_ref()) {
                                    if let Ok(buf) = ts_archive.read_file(&mpq_path) {
                                        debug!("lookup_file: FOUND {relative_path} in map's {ts_mpq}");
                                        let label = format!("{{MAP}}\\{}\\{}", ts_mpq, mpq_path);
                                        return Some((buf, label, relative_path.into()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ── 2–7. Game folder + MPQ chain ─────────────────────────────
    let game_path = get_game_path();
    if game_path.is_empty() {
        debug!("lookup_file: game path is empty, cannot search for {relative_path}");
        return None;
    }
    let game_dir = Path::new(&game_path);

    // 2. {tileset}.mpq (pre-discovered: loose file on disk or extracted from War3*.mpq)
    if let Some(ts) = tileset {
        if !ts.is_empty() {
            let ch = ts.chars().next().unwrap_or('_').to_ascii_uppercase();
            let ts_mpq = format!("{ch}.mpq");
            if let Some(ts_path) = super::game_path::get_tileset_mpq(ts) {
                debug!("lookup_file: trying {relative_path} in {ts_mpq} ({ts_path})");
                if let Ok(archive) = storm_rs::MpqArchive::open(&ts_path) {
                    if let Ok(buf) = archive.read_file(&mpq_path) {
                        debug!("lookup_file: FOUND {relative_path} in {ts_mpq}");
                        let label = format!("{{GAME}}\\{}\\{}", ts_mpq, mpq_path);
                        return Some((buf, label, relative_path.into()));
                    }
                }
                debug!("lookup_file: {relative_path} NOT in {ts_mpq}");
            }
        }
    }

    // 3. Direct file on disk
    let disk_path = game_dir.join(&fs_path);
    debug!("lookup_file: trying {relative_path} on disk at {}", disk_path.display());
    if disk_path.is_file() {
        if let Ok(buf) = std::fs::read(&disk_path) {
            debug!("lookup_file: FOUND {relative_path} on disk");
            let label = format!("{{GAME}}\\{}", mpq_path);
            return Some((buf, label, relative_path.into()));
        }
    }

    // 4–7. MPQ archives: standard chain (tileset already checked above)
    for &mpq_name in MPQ_SEARCH_ORDER {
        let mpq_file = game_dir.join(mpq_name);
        if !mpq_file.exists() {
            debug!("lookup_file: skipping {mpq_name} (not found at {})", mpq_file.display());
            continue;
        }
        debug!("lookup_file: trying {relative_path} in {mpq_name}");
        if let Ok(archive) = storm_rs::MpqArchive::open(mpq_file.to_string_lossy().as_ref()) {
            if let Ok(buf) = archive.read_file(&mpq_path) {
                debug!("lookup_file: FOUND {relative_path} in {mpq_name}");
                let label = format!("{{GAME}}\\{}\\{}", mpq_name, mpq_path);
                return Some((buf, label, relative_path.into()));
            }
        }
        debug!("lookup_file: {relative_path} NOT in {mpq_name}");
    }


    None
}

/// Check whether `relative_path` exists anywhere in the cascade (without
/// reading the contents).  Returns `true` on first hit.
/// Automatically includes the global tileset MPQ.
pub fn lookup_file_exists(relative_path: &str, archive_path: Option<&str>) -> bool {
    let ts = super::game_path::get_tileset();
    lookup_file_exists_ext(relative_path, archive_path, ts.as_deref())
}

/// Like `lookup_file_exists`, but with an optional tileset.
pub fn lookup_file_exists_ext(relative_path: &str, archive_path: Option<&str>, tileset: Option<&str>) -> bool {
    // Try the exact path first (with extension fallbacks).
    if exists_with_ext_fallback(relative_path, archive_path, tileset) {
        return true;
    }

    // Variation fallback: strip trailing digits from the filename stem and retry.
    if let Some(base) = strip_variation_digits(relative_path) {
        return exists_with_ext_fallback(&base, archive_path, tileset);
    }

    false
}

/// Core existence check with extension fallback (.mdx↔.mdl, .tga↔.blp)
/// but **without** variation digit stripping.
fn exists_with_ext_fallback(relative_path: &str, archive_path: Option<&str>, tileset: Option<&str>) -> bool {
    let lower = relative_path.to_ascii_lowercase();

    // ── Model extension normalization: always try .mdx first, then .mdl ──
    if lower.ends_with(".mdx") || lower.ends_with(".mdl") {
        let base = &relative_path[..relative_path.len() - 4];
        if exists_cascade(&format!("{base}.mdx"), archive_path, tileset) {
            return true;
        }
        return exists_cascade(&format!("{base}.mdl"), archive_path, tileset);
    }

    // ── Texture extension normalization: always try .tga first, then .blp ──
    if lower.ends_with(".tga") || lower.ends_with(".blp") {
        let base = &relative_path[..relative_path.len() - 4];
        if exists_cascade(&format!("{base}.tga"), archive_path, tileset) {
            return true;
        }
        return exists_cascade(&format!("{base}.blp"), archive_path, tileset);
    }

    // ── No extension: try as model (.mdx, .mdl), then as texture (.tga, .blp), then exact ──
    let last_sep = lower.rfind(['/', '\\']).unwrap_or(0);
    if !lower[last_sep..].contains('.') {
        if exists_cascade(&format!("{relative_path}.mdx"), archive_path, tileset) { return true; }
        if exists_cascade(&format!("{relative_path}.mdl"), archive_path, tileset) { return true; }
        if exists_cascade(&format!("{relative_path}.tga"), archive_path, tileset) { return true; }
        if exists_cascade(&format!("{relative_path}.blp"), archive_path, tileset) { return true; }
    }

    exists_cascade(relative_path, archive_path, tileset)
}

/// Internal: check existence in the full cascade (no extension fallbacks).
fn exists_cascade(relative_path: &str, archive_path: Option<&str>, tileset: Option<&str>) -> bool {
    let mpq_path = relative_path.replace('/', "\\");
    let fs_path = relative_path.replace('\\', "/");

    // 1. Map archive
    if let Some(ap) = archive_path {
        if let Ok(archive) = storm_rs::MpqArchive::open(ap) {
            // 1a. Direct file in map archive.
            if archive.read_file(&mpq_path).is_ok() {
                return true;
            }

            // 1b. Tileset MPQ inside map archive.
            if let Some(ts) = tileset {
                if !ts.is_empty() {
                    let ch = ts.chars().next().unwrap_or('_').to_ascii_uppercase();
                    let ts_mpq = format!("{ch}.mpq");
                    if let Ok(ts_buf) = archive.read_file(&ts_mpq) {
                        let temp_dir = std::env::temp_dir().join("jass-tree-sitter").join("map-tileset");
                        if std::fs::create_dir_all(&temp_dir).is_ok() {
                            let temp_path = temp_dir.join(&ts_mpq);
                            if std::fs::write(&temp_path, &ts_buf).is_ok() {
                                if let Ok(ts_archive) = storm_rs::MpqArchive::open(temp_path.to_string_lossy().as_ref()) {
                                    if ts_archive.read_file(&mpq_path).is_ok() {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2–7. Game folder + MPQ chain
    let game_path = get_game_path();
    if game_path.is_empty() {
        return false;
    }
    let game_dir = Path::new(&game_path);

    // 2. {tileset}.mpq (pre-discovered)
    if let Some(ts) = tileset {
        if !ts.is_empty() {
            if let Some(ts_path) = super::game_path::get_tileset_mpq(ts) {
                if let Ok(archive) = storm_rs::MpqArchive::open(&ts_path) {
                    if archive.read_file(&mpq_path).is_ok() {
                        return true;
                    }
                }
            }
        }
    }

    // 3. Direct file on disk
    if game_dir.join(&fs_path).is_file() {
        return true;
    }

    // 4–7. MPQ archives: standard chain (tileset already checked above)
    for &mpq_name in MPQ_SEARCH_ORDER {
        let mpq_file = game_dir.join(mpq_name);
        if !mpq_file.exists() {
            continue;
        }
        if let Ok(archive) = storm_rs::MpqArchive::open(mpq_file.to_string_lossy().as_ref()) {
            if archive.read_file(&mpq_path).is_ok() {
                return true;
            }
        }
    }

    false
}

/// Strip trailing ASCII digits from the filename stem.
///
/// Used for doodad/destructable variation fallback: the game appends a
/// variation index (`0`, `1`, …) to the base model path.  If the
/// variation-specific file doesn't exist, the engine falls back to the
/// base path without the digit suffix.
///
/// Examples:
/// * `"Doodads\\grass1"` → `Some("Doodads\\grass")`
/// * `"Doodads\\grass1.mdx"` → `Some("Doodads\\grass.mdx")`
/// * `"Doodads\\grass"` → `None`  (no trailing digits)
/// * `"123"` → `None`  (entire stem is digits — not a variation)
fn strip_variation_digits(path: &str) -> Option<String> {
    let last_sep = path.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    let filename = &path[last_sep..];

    // Split filename into stem and extension.
    let (stem, ext) = match filename.rfind('.') {
        Some(dot) => (&filename[..dot], &filename[dot..]),
        None => (filename, ""),
    };

    // Strip trailing digits from the stem.
    let trimmed = stem.trim_end_matches(|c: char| c.is_ascii_digit());
    if trimmed.len() == stem.len() || trimmed.is_empty() {
        return None; // No trailing digits, or the entire stem is digits.
    }

    Some(format!("{}{}{}", &path[..last_sep], trimmed, ext))
}

