//! Build command: AngelScript output (stub).
//!
//! AS has its own dedicated builder planned. For now this module only keeps
//! compatibility endpoints and returns a stub result.

use url::Url;

use super::project::collect_project;
use crate::http::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
use crate::http::range::Range;
use crate::lng::jass::builder::{BuildOptions, BuildResult, BuilderReport, PipelineMode};

// ─── Public entry point ───────────────────────────────────────────────────────

/// Execute the AngelScript builder with explicit pipeline options.
pub fn run_with_options(uri: &Url, options: BuildOptions) -> BuildResult {
    run_report_with_options(uri, options).result
}

/// Execute the AngelScript builder and return the extended pipeline report.
pub fn run_report_with_options(uri: &Url, options: BuildOptions) -> BuilderReport {
    let project = match collect_project(uri, "build-as", "war3map.as") {
        Ok(p) => p,
        Err(e) => {
            return BuilderReport {
                result: e,
                diagnostics: Vec::new(),
                files: 0,
                functions: 0,
                globals: 0,
                preview: None,
                applied_fixes: Vec::new(),
            }
        }
    };

    let diagnostics = stub_diagnostics();
    let out = transform_as_stub();
    let files = project.files.len();

    if options.mode == PipelineMode::Diagnostics {
        return BuilderReport {
            result: BuildResult::ok(
                String::new(),
                format!("as diagnostics: {} file(s), {} diagnostic(s)", files, diagnostics.len()),
            ),
            diagnostics,
            files,
            functions: 0,
            globals: 0,
            preview: None,
            applied_fixes: Vec::new(),
        };
    }

    if !options.write_output {
        return BuilderReport {
            result: BuildResult::ok(
                String::new(),
                format!("build-as preview: {} file(s)", files),
            ),
            diagnostics,
            files,
            functions: 0,
            globals: 0,
            preview: Some(out),
            applied_fixes: Vec::new(),
        };
    }

    if let Some(parent) = project.out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let result = match std::fs::write(&project.out_path, out) {
        Ok(_) => BuildResult::ok(
            project.out_path.display().to_string(),
            format!("build-as: wrote merged output from {} file(s)", files),
        ),
        Err(e) => BuildResult::err(&crate::util::i18n::build_write_failed(
            &project.out_path.display().to_string(),
            &e.to_string(),
        )),
    };

    BuilderReport {
        result,
        diagnostics,
        files,
        functions: 0,
        globals: 0,
        preview: None,
        applied_fixes: Vec::new(),
    }
}

fn stub_diagnostics() -> Vec<Diagnostic> {
    vec![Diagnostic {
        range: Range::default(),
        message: "AS builder is not implemented yet; using stub".to_string(),
        severity: Some(DiagnosticSeverity::Information),
        source: Some("build".to_string()),
        code: Some(DiagnosticCode::String("as-builder-stub".to_string())),
        ..Default::default()
    }]
}

fn transform_as_stub() -> String {
    "// AS builder stub: not implemented yet\n".to_string()
}

