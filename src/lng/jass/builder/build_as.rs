//! Build command: AngelScript output (stub).
//!
//! Not yet implemented. Creates the output file with a single comment line.

use url::Url;

use super::project::{collect_project, ProjectAst};
use crate::lng::jass::builder::{BuildOptions, BuildResult, PipelineMode};

// ─── Public entry point ───────────────────────────────────────────────────────

/// Execute the AngelScript builder with explicit pipeline options.
pub fn run_with_options(uri: &Url, options: BuildOptions) -> BuildResult {
    let project = match collect_project(uri, "build-as", "war3map.as") {
        Ok(p) => p,
        Err(e) => return e,
    };

    // AS conversion works on a clone of the collected project snapshot.
    let as_project = project.clone();
    let out = transform_as(&as_project, options.mode);

    if options.mode == PipelineMode::Diagnostics {
        return BuildResult::ok(
            String::new(),
            format!("as diagnostics: {} file(s)", project.files.len()),
        );
    }

    if !options.write_output {
        return BuildResult::ok(String::new(), "build-as preview: stub generated".to_string());
    }

    if let Some(parent) = project.out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match std::fs::write(&project.out_path, out) {
        Ok(_) => BuildResult::ok(
            project.out_path.display().to_string(),
            "build-as: stub created".to_string(),
        ),
        Err(e) => BuildResult::err(&crate::util::i18n::build_write_failed(
            &project.out_path.display().to_string(),
            &e.to_string(),
        )),
    }
}

fn transform_as(_project: &ProjectAst, _mode: PipelineMode) -> String {
    "// not implemented yet\n".to_string()
}

