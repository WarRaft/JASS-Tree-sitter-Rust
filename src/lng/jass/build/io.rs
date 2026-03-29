//! File I/O, archive writing, path resolution, and file collection.

use super::ir::{Fragments, FrozenImportEntry};
use super::BuildMode;
use super::BuildResult;
use crate::util::import_graph::IMPORT_GRAPH;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use url::Url;

/// Emit the frozen import header for the build output.
///
/// For each frozen import directive (`//import!` / `//import-ujapi!`),
/// compute a relative path from `out_dir` to the frozen file and emit
/// the directive line.  Returns the header text (empty when there are no
/// frozen imports).
pub(super) fn emit_frozen_import_header(
    entries: &[FrozenImportEntry],
    out_dir: &Path,
) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut header = String::new();

    for entry in entries {
        let frozen_path = match entry.url.to_file_path() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Compute relative path from the output directory to the frozen file.
        let rel = relative_path(out_dir, &frozen_path);

        let directive = if entry.is_ujapi {
            "//import-ujapi!"
        } else {
            "//import!"
        };

        header.push_str(directive);
        header.push(' ');
        // Always use forward slashes in the directive.
        header.push_str(&rel.replace('\\', "/"));
        header.push('\n');
    }
    header.push('\n');

    header
}

/// Compute a relative path from `from_dir` to `to_file`.
///
/// Both paths should be absolute.  The result uses forward slashes.
fn relative_path(from_dir: &Path, to_file: &Path) -> String {
    // Canonicalize both paths as much as possible.
    let from = from_dir
        .canonicalize()
        .unwrap_or_else(|_| from_dir.to_path_buf());
    let to = to_file
        .canonicalize()
        .unwrap_or_else(|_| to_file.to_path_buf());

    // Find common prefix.
    let from_parts: Vec<_> = from.components().collect();
    let to_parts: Vec<_> = to.components().collect();

    let common = from_parts
        .iter()
        .zip(to_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut result = String::new();
    // Go up from from_dir for each remaining component.
    for _ in common..from_parts.len() {
        if !result.is_empty() {
            result.push('/');
        }
        result.push_str("..");
    }
    // Go down to to_file.
    for part in &to_parts[common..] {
        if !result.is_empty() {
            result.push('/');
        }
        result.push_str(&part.as_os_str().to_string_lossy());
    }

    if result.is_empty() {
        // Same directory — just the filename.
        to_file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        result
    }
}

/// Write the build output to a plain file.
pub(super) fn write_output(
    out_path: &Path,
    out: &str,
    sorted_funcs: &[String],
    fragments: &Fragments,
) -> BuildResult {
    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(out_path, out) {
        Ok(_) => BuildResult {
            ok: true,
            path: out_path.display().to_string(),
            message: crate::util::i18n::build_ok(
                fragments.globals_out.len(),
                sorted_funcs.len(),
                fragments.bare_stmts.len(),
            ),
        },
        Err(e) => super::err(&crate::util::i18n::build_write_failed(
            &out_path.display().to_string(),
            &e.to_string(),
        )),
    }
}

/// Check whether a path points to a Warcraft III map archive (`.w3x` or `.w3m`).
pub(super) fn is_archive_path(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let lower = ext.to_ascii_lowercase();
            lower == "w3x" || lower == "w3m"
        }
        None => false,
    }
}

/// Write the build output into a `.w3x` / `.w3m` MPQ archive.
///
/// 1. If `//set backup <path>` is specified, create a backup copy first.
/// 2. Open the archive for writing via `storm_rs::MpqArchiveWriter`.
/// 3. For AS builds, remove `war3map.j` (if present) — the map runs AS now.
/// 4. Inject the merged script as `script_name` (e.g. `war3map.j` / `war3map.as`).
/// 5. Finalize the archive.
pub(super) fn write_output_archive(
    archive_path: &Path,
    out: &str,
    sorted_funcs: &[String],
    fragments: &Fragments,
    script_name: &str,
    base_dir: &Path,
    backup_setting: Option<&(Url, String)>,
    mode: BuildMode,
) -> BuildResult {
    // 1. Create backup if configured.
    if let Some((_backup_uri, backup_target)) = backup_setting {
        let backup_path = resolve_backup_path(base_dir, backup_target, archive_path);
        if let Some(parent) = backup_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::copy(archive_path, &backup_path) {
            return super::err(&crate::util::i18n::build_backup_failed(
                &backup_path.display().to_string(),
                &e.to_string(),
            ));
        }
    }

    // 2. Open archive for writing.
    let mut writer = match storm_rs::MpqArchiveWriter::open(archive_path) {
        Ok(w) => w,
        Err(e) => {
            return super::err(&crate::util::i18n::build_archive_open_failed(
                &archive_path.display().to_string(),
                &e.to_string(),
            ));
        }
    };

    // 3. For AS builds, remove the old JASS script if it exists.
    if mode == BuildMode::As {
        // Try both possible locations; ignore "not found" errors.
        let _ = writer.remove_file("war3map.j");
        let _ = writer.remove_file("Scripts\\war3map.j");
    }

    // 4. Inject the script.
    let options = storm_rs::AddFileOptions {
        compress: true,
        ..storm_rs::AddFileOptions::default()
    };
    if let Err(e) = writer.add_file(script_name, out.as_bytes(), &options) {
        return super::err(&crate::util::i18n::build_archive_inject_failed(
            script_name,
            &e.to_string(),
        ));
    }

    // 5. Finalize.
    if let Err(e) = writer.finish() {
        return super::err(&crate::util::i18n::build_archive_inject_failed(
            script_name,
            &e.to_string(),
        ));
    }

    // 6. Save the generated script to backup directory with date prefix.
    if let Some((_backup_uri, backup_target)) = backup_setting {
        let script_pseudo_path = Path::new(script_name);
        let script_backup = resolve_backup_path(base_dir, backup_target, script_pseudo_path);
        if let Some(parent) = script_backup.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Best-effort — don't fail the build if this write fails.
        let _ = std::fs::write(&script_backup, out);
    }

    BuildResult {
        ok: true,
        path: archive_path.display().to_string(),
        message: crate::util::i18n::build_archive_ok(
            fragments.globals_out.len(),
            sorted_funcs.len(),
            fragments.bare_stmts.len(),
            script_name,
        ),
    }
}

/// Resolve the backup destination path.
///
/// The backup file is always placed with a timestamp-prefixed name:
/// `YYYY_MM_DD_HH_MM_OriginalFileName.ext` (e.g. `2026_03_23_14_05_MyMap.w3x`).
///
/// If the target is a directory (ends with `/` or has no extension),
/// the timestamp-prefixed archive filename is placed into that directory.
/// Otherwise, the target is treated as the full backup file path
/// (the timestamp prefix is still prepended to the filename component).
fn resolve_backup_path(base_dir: &Path, target: &str, archive_path: &Path) -> PathBuf {
    let raw = target.replace('\\', "/");
    let p = Path::new(&raw);

    let resolved = if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    };

    // Build the timestamp prefix (YYYY_MM_DD_HH_MM_).
    let ts_prefix = {
        use std::time::SystemTime;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let days = (now / 86400) as i64;
        let (y, m, d) = civil_from_days(days);
        let day_secs = now % 86400;
        let hh = day_secs / 3600;
        let mm = (day_secs % 3600) / 60;
        format!("{:04}_{:02}_{:02}_{:02}_{:02}_", y, m, d, hh, mm)
    };

    // If it looks like a directory, append the timestamped archive filename.
    if raw.ends_with('/') || resolved.extension().is_none() {
        let orig_name = archive_path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("backup.w3x"))
            .to_string_lossy();
        let dated_name = format!("{}{}", ts_prefix, orig_name);
        resolved.join(dated_name)
    } else {
        // Target is a specific file path — prepend timestamp to its filename.
        let dir = resolved.parent().unwrap_or(Path::new("."));
        let orig_name = resolved
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("backup.w3x"))
            .to_string_lossy();
        let dated_name = format!("{}{}", ts_prefix, orig_name);
        dir.join(dated_name)
    }
}

/// Convert a day count since the Unix epoch to (year, month, day).
///
/// Uses Howard Hinnant's civil_from_days algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Resolve `<path>` relative to `base_dir`. If `<path>` looks like a directory
/// (ends with `/` or `\` or has no extension), append `default_file`.
pub(super) fn resolve_output_path(base_dir: &Path, target: &str, default_file: &str) -> PathBuf {
    let raw = target.replace('\\', "/");
    let p = Path::new(&raw);

    let resolved = if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    };

    // If it looks like a directory, append default filename.
    if raw.ends_with('/') || resolved.extension().is_none() {
        resolved.join(default_file)
    } else {
        resolved
    }
}

/// Collect the ordered list of file URIs to process for a build.
///
/// The `trigger_uri` is the `//entry` file that owns the `//set build-*`
/// directive.  The file order is the set of files transitively imported by
/// `trigger_uri` (forward-only BFS), with the entry file itself appended
/// last so its bare top-level statements end up in `main`.
/// Files not reachable from the entry are excluded (tree-shaking).
pub(super) fn collect_file_order(trigger_uri: &Url) -> Vec<Url> {
    let mut deps = IMPORT_GRAPH.dependencies(trigger_uri);
    // Put the trigger file last (its bare statements go into main).
    deps.push(trigger_uri.clone());
    // Deduplicate while preserving order.
    let mut seen = HashSet::new();
    deps.retain(|u| seen.insert(u.clone()));
    deps
}

/// Read file source: from ROPE_MAP if open, otherwise from disk.
pub(super) fn read_file_source(uri: &Url) -> Option<String> {
    use crate::util::roper::uri_map::ROPE_MAP;
    if let Some(rope) = ROPE_MAP.get(uri) {
        return Some(rope.to_string());
    }
    let path = uri.to_file_path().ok()?;
    std::fs::read_to_string(&path).ok()
}

