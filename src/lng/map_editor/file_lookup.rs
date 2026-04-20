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
//!
//! The main entry-point is [`Lookuper`]: create it once per request with the
//! map archive path and tileset, then call its methods for every file search.
//! All MPQ archive handles are opened once and reused, so a bulk operation
//! (e.g. resolving thousands of model variants) pays the `open()` cost only once.
//!
//! The stand-alone free functions (`lookup_file`, `lookup_file_exists`, …)
//! are kept for backward-compatibility; they simply create a temporary
//! `Lookuper` on each call.

use crate::lng::map_editor::game_path::get_game_path;
use log::debug;
use serde::Serialize;

/// MPQ archives to search after the tileset MPQ (in priority order).
const MPQ_SEARCH_ORDER: &[&str] = &[
    "War3Patch.mpq",
    "War3xLocal.mpq",
    "War3x.mpq",
    "War3.mpq",
];

// ─── ModelVariantFound ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ModelVariantFound {
    pub path: String,
    pub resolved_path: String,
    pub source: String,
}

// ─── Lookuper ─────────────────────────────────────────────────────────────────

/// A configured file-search context that keeps all MPQ archive handles open.
///
/// Create once per request, then call its methods for every file lookup.
/// This avoids re-opening the same MPQ archives on every individual check.
///
/// ```rust
/// let lu = Lookuper::new(Some(archive_path), tileset.as_deref());
/// let data = lu.lookup("TerrainArt\\Terrain.slk");
/// let variants = lu.resolve_model_variants("Doodads\\Cinematic\\FlameStrike");
/// ```
pub struct Lookuper {
    /// 1a – open map archive handle (optional)
    map_archive: Option<storm_rs::MpqArchive>,
    /// 1b – tileset MPQ extracted from the map archive  (label_prefix, archive)
    map_tileset_archive: Option<(String, storm_rs::MpqArchive)>,
    /// game directory (None when game path is not configured)
    game_dir: Option<std::path::PathBuf>,
    /// 2 – game tileset MPQ  (label, archive)
    game_tileset_archive: Option<(String, storm_rs::MpqArchive)>,
    /// 4-7 – War3Patch / War3xLocal / War3x / War3  (label, archive)
    game_mpqs: Vec<(String, storm_rs::MpqArchive)>,
}

impl Lookuper {
    // ── Construction ───────────────────────────────────────────────────────────

    /// Build a fully explicit `Lookuper`.
    ///
    /// - `archive_path` – absolute path to the `.w3x`/`.w3m` map archive (or `None`).
    /// - `tileset`      – one-letter tileset code (e.g. `"L"`), or `None`.
    /// - `game_path`    – absolute path to the Warcraft III installation directory (or `None`).
    ///
    /// All archive handles are opened here; subsequent lookups just reuse them.
    pub fn new(
        archive_path: Option<&str>,
        tileset: Option<&str>,
        game_path: Option<&str>,
    ) -> Self {
        // 1a. Open the map archive once.
        let map_archive = archive_path.and_then(|ap| storm_rs::MpqArchive::open(ap).ok());

        // 1b. Tileset MPQ embedded inside the map archive.
        let map_tileset_archive: Option<(String, storm_rs::MpqArchive)> = (|| {
            let ts = tileset?;
            if ts.is_empty() {
                return None;
            }
            let ch = ts.chars().next()?.to_ascii_uppercase();
            let ts_mpq = format!("{ch}.mpq");
            let ts_buf = map_archive.as_ref()?.read_file(&ts_mpq).ok()?;
            let temp_dir = std::env::temp_dir()
                .join("jass-tree-sitter")
                .join("map-tileset");
            std::fs::create_dir_all(&temp_dir).ok()?;
            let temp_path = temp_dir.join(&ts_mpq);
            std::fs::write(&temp_path, &ts_buf).ok()?;
            let archive =
                storm_rs::MpqArchive::open(temp_path.to_string_lossy().as_ref()).ok()?;
            Some((format!("{{MAP}}\\{ts_mpq}"), archive))
        })();

        // Game directory.
        let game_dir = game_path
            .filter(|p| !p.is_empty())
            .map(std::path::PathBuf::from);

        // 2. Game tileset MPQ.
        let game_tileset_archive: Option<(String, storm_rs::MpqArchive)> = (|| {
            let ts = tileset?;
            if ts.is_empty() {
                return None;
            }
            let ch = ts.chars().next()?.to_ascii_uppercase();
            let ts_mpq = format!("{ch}.mpq");
            let ts_path = super::game_path::get_tileset_mpq(ts)?;
            let archive = storm_rs::MpqArchive::open(&ts_path).ok()?;
            Some((format!("{{GAME}}\\{ts_mpq}"), archive))
        })();

        // 4-7. Standard War3* MPQ chain.
        let mut game_mpqs: Vec<(String, storm_rs::MpqArchive)> = Vec::new();
        if let Some(ref gdir) = game_dir {
            for &mpq_name in MPQ_SEARCH_ORDER {
                let mpq_file = gdir.join(mpq_name);
                if !mpq_file.exists() {
                    continue;
                }
                if let Ok(archive) =
                    storm_rs::MpqArchive::open(mpq_file.to_string_lossy().as_ref())
                {
                    game_mpqs.push((mpq_name.to_string(), archive));
                }
            }
        }

        Self {
            map_archive,
            map_tileset_archive,
            game_dir,
            game_tileset_archive,
            game_mpqs,
        }
    }

    /// Convenience constructor that reads both tileset and game path from global state.
    pub fn from_archive(archive_path: Option<&str>) -> Self {
        let ts = super::game_path::get_tileset();
        let gp = get_game_path();
        Self::new(
            archive_path,
            ts.as_deref(),
            if gp.is_empty() { None } else { Some(gp.as_str()) },
        )
    }

    // ── Map-archive direct access ──────────────────────────────────────────────

    /// Read a file directly from the map archive (no cascade, no ext fallback).
    /// Returns `None` if the map archive is not open or the file is absent.
    pub fn read_map_file(&self, path: &str) -> Option<Vec<u8>> {
        let mpq_path = path.replace('/', "\\");
        self.map_archive.as_ref()?.read_file(&mpq_path).ok()
    }

    // ── Core cascade ───────────────────────────────────────────────────────────

    /// Search the full cascade for exactly `relative_path` (no extension fallbacks).
    /// Returns `(bytes, source_label, resolved_path)` on success.
    fn lookup_exact(&self, relative_path: &str) -> Option<(Vec<u8>, String, String)> {
        let mpq_path = relative_path.replace('/', "\\");
        let fs_path = relative_path.replace('\\', "/");

        // 1a. Map archive.
        if let Some(ref archive) = self.map_archive {
            if let Ok(buf) = archive.read_file(&mpq_path) {
                debug!("Lookuper: FOUND {relative_path} in map archive");
                return Some((buf, format!("{{MAP}}\\{mpq_path}"), relative_path.into()));
            }
        }

        // 1b. Tileset MPQ inside map archive.
        if let Some((ref label_pfx, ref archive)) = self.map_tileset_archive {
            if let Ok(buf) = archive.read_file(&mpq_path) {
                debug!("Lookuper: FOUND {relative_path} in map tileset archive");
                return Some((
                    buf,
                    format!("{label_pfx}\\{mpq_path}"),
                    relative_path.into(),
                ));
            }
        }

        // Need game_dir for everything below.
        let game_dir = self.game_dir.as_deref()?;

        // 2. Game tileset MPQ.
        if let Some((ref label, ref archive)) = self.game_tileset_archive {
            if let Ok(buf) = archive.read_file(&mpq_path) {
                debug!("Lookuper: FOUND {relative_path} in game tileset archive");
                return Some((buf, format!("{label}\\{mpq_path}"), relative_path.into()));
            }
        }

        // 3. Disk.
        let disk_path = game_dir.join(&fs_path);
        if disk_path.is_file() {
            if let Ok(buf) = std::fs::read(&disk_path) {
                debug!("Lookuper: FOUND {relative_path} on disk");
                return Some((buf, format!("{{GAME}}\\{mpq_path}"), relative_path.into()));
            }
        }

        // 4-7. War3* MPQ chain.
        for (label, archive) in &self.game_mpqs {
            if let Ok(buf) = archive.read_file(&mpq_path) {
                debug!("Lookuper: FOUND {relative_path} in {label}");
                return Some((
                    buf,
                    format!("{{GAME}}\\{label}\\{mpq_path}"),
                    relative_path.into(),
                ));
            }
        }

        None
    }

    /// Check existence in the full cascade (no extension fallbacks).
    fn exists_exact(&self, relative_path: &str) -> bool {
        let mpq_path = relative_path.replace('/', "\\");
        let fs_path = relative_path.replace('\\', "/");

        if let Some(ref a) = self.map_archive {
            if a.read_file(&mpq_path).is_ok() {
                return true;
            }
        }
        if let Some((_, ref a)) = self.map_tileset_archive {
            if a.read_file(&mpq_path).is_ok() {
                return true;
            }
        }
        let Some(game_dir) = self.game_dir.as_deref() else {
            return false;
        };
        if let Some((_, ref a)) = self.game_tileset_archive {
            if a.read_file(&mpq_path).is_ok() {
                return true;
            }
        }
        if game_dir.join(&fs_path).is_file() {
            return true;
        }
        for (_, a) in &self.game_mpqs {
            if a.read_file(&mpq_path).is_ok() {
                return true;
            }
        }
        false
    }

    // ── Extension-fallback helpers ─────────────────────────────────────────────

    fn lookup_with_ext_fallback(
        &self,
        relative_path: &str,
    ) -> Option<(Vec<u8>, String, String)> {
        let lower = relative_path.to_ascii_lowercase();

        if lower.ends_with(".mdx") || lower.ends_with(".mdl") {
            let base = &relative_path[..relative_path.len() - 4];
            if let r @ Some(_) = self.lookup_exact(&format!("{base}.mdx")) {
                return r;
            }
            return self.lookup_exact(&format!("{base}.mdl"));
        }

        if lower.ends_with(".tga") || lower.ends_with(".blp") {
            let base = &relative_path[..relative_path.len() - 4];
            if let r @ Some(_) = self.lookup_exact(&format!("{base}.tga")) {
                return r;
            }
            return self.lookup_exact(&format!("{base}.blp"));
        }

        let last_sep = lower.rfind(['/', '\\']).unwrap_or(0);
        if !lower[last_sep..].contains('.') {
            if let r @ Some(_) = self.lookup_exact(&format!("{relative_path}.mdx")) {
                return r;
            }
            if let r @ Some(_) = self.lookup_exact(&format!("{relative_path}.mdl")) {
                return r;
            }
            if let r @ Some(_) = self.lookup_exact(&format!("{relative_path}.tga")) {
                return r;
            }
            if let r @ Some(_) = self.lookup_exact(&format!("{relative_path}.blp")) {
                return r;
            }
        }

        self.lookup_exact(relative_path)
    }

    fn lookup_model_with_ext_fallback(
        &self,
        relative_path: &str,
    ) -> Option<(Vec<u8>, String, String)> {
        let lower = relative_path.to_ascii_lowercase();

        if lower.ends_with(".mdx") || lower.ends_with(".mdl") {
            let base = &relative_path[..relative_path.len() - 4];
            if let r @ Some(_) = self.lookup_exact(&format!("{base}.mdx")) {
                return r;
            }
            return self.lookup_exact(&format!("{base}.mdl"));
        }

        let last_sep = lower.rfind(['/', '\\']).unwrap_or(0);
        if !lower[last_sep..].contains('.') {
            if let r @ Some(_) = self.lookup_exact(&format!("{relative_path}.mdx")) {
                return r;
            }
            if let r @ Some(_) = self.lookup_exact(&format!("{relative_path}.mdl")) {
                return r;
            }
        }

        self.lookup_exact(relative_path)
    }

    fn lookup_texture_with_ext_fallback(
        &self,
        relative_path: &str,
    ) -> Option<(Vec<u8>, String, String)> {
        let lower = relative_path.to_ascii_lowercase();

        if lower.ends_with(".tga") || lower.ends_with(".blp") {
            let base = &relative_path[..relative_path.len() - 4];
            if let r @ Some(_) = self.lookup_exact(&format!("{base}.tga")) {
                return r;
            }
            return self.lookup_exact(&format!("{base}.blp"));
        }

        let last_sep = lower.rfind(['/', '\\']).unwrap_or(0);
        if !lower[last_sep..].contains('.') {
            if let r @ Some(_) = self.lookup_exact(&format!("{relative_path}.tga")) {
                return r;
            }
            if let r @ Some(_) = self.lookup_exact(&format!("{relative_path}.blp")) {
                return r;
            }
        }

        self.lookup_exact(relative_path)
    }

    fn exists_with_ext_fallback(&self, relative_path: &str) -> bool {
        let lower = relative_path.to_ascii_lowercase();

        if lower.ends_with(".mdx") || lower.ends_with(".mdl") {
            let base = &relative_path[..relative_path.len() - 4];
            if self.exists_exact(&format!("{base}.mdx")) {
                return true;
            }
            return self.exists_exact(&format!("{base}.mdl"));
        }

        if lower.ends_with(".tga") || lower.ends_with(".blp") {
            let base = &relative_path[..relative_path.len() - 4];
            if self.exists_exact(&format!("{base}.tga")) {
                return true;
            }
            return self.exists_exact(&format!("{base}.blp"));
        }

        let last_sep = lower.rfind(['/', '\\']).unwrap_or(0);
        if !lower[last_sep..].contains('.') {
            if self.exists_exact(&format!("{relative_path}.mdx")) {
                return true;
            }
            if self.exists_exact(&format!("{relative_path}.mdl")) {
                return true;
            }
            if self.exists_exact(&format!("{relative_path}.tga")) {
                return true;
            }
            if self.exists_exact(&format!("{relative_path}.blp")) {
                return true;
            }
        }

        self.exists_exact(relative_path)
    }

    // ── Public lookup API ──────────────────────────────────────────────────────

    /// Full generic lookup: extension fallback + variation-digit fallback.
    ///
    /// Returns `(bytes, source_label, resolved_path)`.
    pub fn lookup(&self, relative_path: &str) -> Option<(Vec<u8>, String, String)> {
        if let r @ Some(_) = self.lookup_with_ext_fallback(relative_path) {
            return r;
        }
        if let Some(base) = strip_variation_digits(relative_path) {
            debug!("Lookuper::lookup: variation fallback {relative_path} → {base}");
            return self.lookup_with_ext_fallback(&base);
        }
        None
    }

    /// Model-only lookup (`.mdx` / `.mdl`) with variation-digit fallback.
    pub fn lookup_model(&self, relative_path: &str) -> Option<(Vec<u8>, String, String)> {
        if let r @ Some(_) = self.lookup_model_with_ext_fallback(relative_path) {
            return r;
        }
        if let Some(base) = strip_variation_digits(relative_path) {
            debug!("Lookuper::lookup_model: variation fallback {relative_path} → {base}");
            return self.lookup_model_with_ext_fallback(&base);
        }
        None
    }

    /// Texture-only lookup (`.tga` / `.blp`).
    pub fn lookup_texture(&self, relative_path: &str) -> Option<(Vec<u8>, String, String)> {
        self.lookup_texture_with_ext_fallback(relative_path)
    }

    /// Dispatch lookup by `kind` (`"model"`, `"texture"`, or generic).
    pub fn lookup_kind(
        &self,
        relative_path: &str,
        kind: Option<&str>,
    ) -> Option<(Vec<u8>, String, String)> {
        match kind.unwrap_or("").to_ascii_lowercase().as_str() {
            "model" => self.lookup_model(relative_path),
            "texture" => self.lookup_texture(relative_path),
            _ => self.lookup(relative_path),
        }
    }

    /// Check existence with extension fallback + variation-digit fallback.
    pub fn exists(&self, relative_path: &str) -> bool {
        if self.exists_with_ext_fallback(relative_path) {
            return true;
        }
        if let Some(base) = strip_variation_digits(relative_path) {
            return self.exists_with_ext_fallback(&base);
        }
        false
    }

    /// Resolve all existing model variants for a doodad/destructable model path.
    ///
    /// Probes `stem0`..`stem9` (where `stem` is derived from `search_path`).
    /// If at least one digit variant is found, returns only those.
    /// Otherwise falls back to the base stem without a digit suffix.
    pub fn resolve_model_variants(&self, search_path: &str) -> Vec<ModelVariantFound> {
        let stem = model_variant_stem(search_path);
        if stem.is_empty() {
            return Vec::new();
        }

        let mut digit_variants: Vec<ModelVariantFound> = Vec::new();
        for i in 0..10u32 {
            let candidate = format!("{stem}{i}");
            if let Some((_buf, source, resolved_path)) =
                self.lookup_model_with_ext_fallback(&candidate)
            {
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

        if let Some((_buf, source, resolved_path)) =
            self.lookup_model_with_ext_fallback(&stem)
        {
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
}

// ─── Free-function shims (backward compatibility) ─────────────────────────────
//
// Each function creates a temporary Lookuper.  Code that calls these functions
// in a tight loop should be updated to create one Lookuper and reuse it.

/// Find `relative_path` using the cascading lookup.
/// Returns `(file_bytes, source_label)` on success.
pub fn lookup_file(relative_path: &str, archive_path: Option<&str>) -> Option<(Vec<u8>, String)> {
    Lookuper::from_archive(archive_path)
        .lookup(relative_path)
        .map(|(b, s, _)| (b, s))
}

/// Like `lookup_file`, but with an explicit tileset override.
pub fn lookup_file_ext(
    relative_path: &str,
    archive_path: Option<&str>,
    tileset: Option<&str>,
) -> Option<(Vec<u8>, String)> {
    let gp = get_game_path();
    Lookuper::new(archive_path, tileset, if gp.is_empty() { None } else { Some(gp.as_str()) })
        .lookup(relative_path)
        .map(|(b, s, _)| (b, s))
}

/// Like `lookup_file`, but also returns the resolved path.
pub fn lookup_file_resolved(
    relative_path: &str,
    archive_path: Option<&str>,
) -> Option<(Vec<u8>, String, String)> {
    Lookuper::from_archive(archive_path).lookup(relative_path)
}

/// Like `lookup_file_resolved`, but with an explicit tileset override.
pub fn lookup_file_resolved_ext(
    relative_path: &str,
    archive_path: Option<&str>,
    tileset: Option<&str>,
) -> Option<(Vec<u8>, String, String)> {
    let gp = get_game_path();
    Lookuper::new(archive_path, tileset, if gp.is_empty() { None } else { Some(gp.as_str()) })
        .lookup(relative_path)
}

/// Kind-aware resolved lookup (`"model"`, `"texture"`, or generic).
pub fn lookup_file_resolved_kind_ext(
    relative_path: &str,
    archive_path: Option<&str>,
    tileset: Option<&str>,
    kind: Option<&str>,
) -> Option<(Vec<u8>, String, String)> {
    let gp = get_game_path();
    Lookuper::new(archive_path, tileset, if gp.is_empty() { None } else { Some(gp.as_str()) })
        .lookup_kind(relative_path, kind)
}

/// Check whether `relative_path` exists anywhere in the cascade.
pub fn lookup_file_exists(relative_path: &str, archive_path: Option<&str>) -> bool {
    Lookuper::from_archive(archive_path).exists(relative_path)
}

/// Like `lookup_file_exists`, but with an explicit tileset override.
pub fn lookup_file_exists_ext(
    relative_path: &str,
    archive_path: Option<&str>,
    tileset: Option<&str>,
) -> bool {
    let gp = get_game_path();
    Lookuper::new(archive_path, tileset, if gp.is_empty() { None } else { Some(gp.as_str()) })
        .exists(relative_path)
}

/// Resolve existing model variants for a doodad/destructable model path.
///
/// Prefer passing a pre-built [`Lookuper`] in performance-sensitive callers.
pub fn resolve_model_variants_ext(
    search_path: &str,
    archive_path: Option<&str>,
    tileset: Option<&str>,
) -> Vec<ModelVariantFound> {
    let gp = get_game_path();
    Lookuper::new(archive_path, tileset, if gp.is_empty() { None } else { Some(gp.as_str()) })
        .resolve_model_variants(search_path)
}

// ─── Private helpers ──────────────────────────────────────────────────────────

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
    let last_sep = search_path
        .rfind(['/', '\\'])
        .map(|i| i + 1)
        .unwrap_or(0);
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

/// Strip trailing ASCII digits from the filename stem.
///
/// * `"Doodads\\grass1"`     → `Some("Doodads\\grass")`
/// * `"Doodads\\grass1.mdx"` → `Some("Doodads\\grass.mdx")`
/// * `"Doodads\\grass"`      → `None`  (no trailing digits)
/// * `"123"`                 → `None`  (entire stem is digits)
fn strip_variation_digits(path: &str) -> Option<String> {
    let last_sep = path.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    let filename = &path[last_sep..];

    let (stem, ext) = match filename.rfind('.') {
        Some(dot) => (&filename[..dot], &filename[dot..]),
        None => (filename, ""),
    };

    let trimmed = stem.trim_end_matches(|c: char| c.is_ascii_digit());
    if trimmed.len() == stem.len() || trimmed.is_empty() {
        return None;
    }

    Some(format!("{}{}{}", &path[..last_sep], trimmed, ext))
}
