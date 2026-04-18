//! JASS builder — assembles the import tree into a single output file.
//!
//! # Rules
//! - **No business logic here.**  `mod.rs` is a public API surface only.
//!   All implementation lives in the sub-modules below.
//! - Each build command (`build_jass`, `build_as`, …) has its own file.
//! - Shared utilities are split into `collect`, `render`, and `sort`.

pub mod collect;
pub mod render;
pub mod sort;
pub mod build_jass;
pub mod build_as;
pub mod local_fix;
pub mod project;
pub mod uglify;

use serde::Serialize;
use url::Url;
use crate::http::diagnostic::Diagnostic;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineMode {
    Diagnostics,
    Build,
}

#[derive(Debug, Clone, Copy)]
pub struct BuildOptions {
    pub mode: PipelineMode,
    pub write_output: bool,
}

impl BuildOptions {
    pub const fn build() -> Self {
        Self {
            mode: PipelineMode::Build,
            write_output: true,
        }
    }

    pub const fn diagnostics() -> Self {
        Self {
            mode: PipelineMode::Diagnostics,
            write_output: false,
        }
    }

    pub const fn build_preview() -> Self {
        Self {
            mode: PipelineMode::Build,
            write_output: false,
        }
    }
}

// ─── Shared result type ───────────────────────────────────────────────────────

/// Outcome of any build command.
#[derive(Debug, Clone, Serialize)]
pub struct BuildResult {
    /// `true` when the file was successfully written.
    pub ok: bool,
    /// Path where the output was written (empty on error).
    pub path: String,
    /// Human-readable message (success description or error detail).
    pub message: String,
}

impl BuildResult {
    pub fn ok(path: String, message: String) -> Self {
        Self { ok: true, path, message }
    }

    pub fn err(message: &str) -> Self {
        Self { ok: false, path: String::new(), message: message.to_string() }
    }
}

/// Extended builder outcome for diagnostics/build pipelines.
#[derive(Debug, Clone, Serialize)]
pub struct BuilderReport {
    pub result: BuildResult,
    pub diagnostics: Vec<Diagnostic>,
    pub files: usize,
    pub functions: usize,
    pub globals: usize,
    pub preview: Option<String>,
    pub applied_fixes: Vec<String>,
}

// ─── Public build commands ────────────────────────────────────────────────────

/// Merge all JASS files in the import tree into a single `.j` file.
pub fn build_jass(uri: &Url) -> BuildResult {
    build_jass::run_with_options(uri, BuildOptions::build())
}

/// Analyze JASS project without writing output.
pub fn diagnose_jass(uri: &Url) -> BuildResult {
    build_jass::run_with_options(uri, BuildOptions::diagnostics())
}

/// Build JASS preview without writing output.
pub fn build_jass_preview(uri: &Url) -> BuildResult {
    build_jass::run_with_options(uri, BuildOptions::build_preview())
}

/// Analyze JASS project and return an extended report.
pub fn diagnose_jass_report(uri: &Url) -> BuilderReport {
    crate::lng::jass::builder::build_jass::run_report_with_options(uri, BuildOptions::diagnostics())
}

/// Build JASS preview and return an extended report.
pub fn build_jass_preview_report(uri: &Url) -> BuilderReport {
    crate::lng::jass::builder::build_jass::run_report_with_options(uri, BuildOptions::build_preview())
}

/// Write an AngelScript stub output file (not yet implemented).
pub fn build_as(uri: &Url) -> BuildResult {
    build_as::run_with_options(uri, BuildOptions::build())
}

/// Analyze AngelScript output generation without writing output.
pub fn diagnose_as(uri: &Url) -> BuildResult {
    build_as::run_with_options(uri, BuildOptions::diagnostics())
}

/// Build AngelScript preview without writing output.
pub fn build_as_preview(uri: &Url) -> BuildResult {
    build_as::run_with_options(uri, BuildOptions::build_preview())
}

/// Analyze AngelScript output generation and return an extended report.
pub fn diagnose_as_report(uri: &Url) -> BuilderReport {
    crate::lng::jass::builder::build_as::run_report_with_options(uri, BuildOptions::diagnostics())
}

/// Build AngelScript preview and return an extended report.
pub fn build_as_preview_report(uri: &Url) -> BuilderReport {
    crate::lng::jass::builder::build_as::run_report_with_options(uri, BuildOptions::build_preview())
}

/// Check whether `key` exists in any `//entry` file of the connected component.
pub fn has_build_setting(uri: &Url, key: &str) -> bool {
    collect::has_build_setting(uri, key)
}

/// Resolve `{{variable}}` hook commands for the build entry of `uri`.
///
/// Returns `(before_cmd, after_cmd, cwd)`.
pub fn resolve_hooks(uri: &Url) -> (String, String, String) {
    collect::resolve_hooks(uri)
}

/// Run local single-file fixes and write the result back to the same file.
pub fn fix_local(uri: &Url) -> BuildResult {
    local_fix::fix_local(uri, true)
}

/// Run local single-file fixes in preview mode (no file write).
pub fn fix_local_preview(uri: &Url) -> BuildResult {
    local_fix::fix_local(uri, false)
}

