//! Lifetime-free project input for builder pipelines.
//!
//! The current parser AST (`ast.rs`) is tied to tree-sitter node lifetimes,
//! so the builder stores the whole project as owned file sources + metadata.
//! Per-file typed ASTs are rebuilt inside pipeline passes from this collected
//! project snapshot.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use url::Url;

use crate::lng::jass::builder::BuildResult;
use crate::util::parse_cache::{is_uri_frozen, peek_or_load};

use super::collect::{collect_file_order, find_build_setting, read_source, resolve_output_path};

#[derive(Debug, Clone)]
pub struct ProjectFile {
    pub uri: Url,
    pub source: String,
    pub is_frozen: bool,
    pub function_callees: HashMap<String, HashSet<String>>,
}

#[derive(Debug, Clone)]
pub struct ProjectAst {
    pub out_path: PathBuf,
    pub files: Vec<ProjectFile>,
}

pub fn collect_project(uri: &Url, build_key: &str, default_file: &str) -> Result<ProjectAst, BuildResult> {
    let (trigger_uri, target) = match find_build_setting(uri, build_key) {
        Some(pair) => pair,
        None => {
            let msg = if build_key == "build-as" {
                crate::util::i18n::build_no_setting_as()
            } else {
                crate::util::i18n::build_no_setting_jass()
            };
            return Err(BuildResult::err(msg));
        }
    };

    let base_dir = match trigger_uri.to_file_path() {
        Ok(p) => match p.parent() {
            Some(d) => d.to_path_buf(),
            None => return Err(BuildResult::err(crate::util::i18n::build_no_parent_dir())),
        },
        Err(_) => return Err(BuildResult::err(crate::util::i18n::build_not_file_path())),
    };

    let out_path = resolve_output_path(&base_dir, &target, default_file);
    let file_order = collect_file_order(&trigger_uri);

    let mut files = Vec::new();
    for file_uri in file_order {
        let source = match read_source(&file_uri) {
            Some(s) => s,
            None => continue,
        };

        let mut function_callees = HashMap::<String, HashSet<String>>::new();
        if let Some(snap) = peek_or_load(&file_uri) {
            for f in &snap.file_symbols.functions {
                function_callees.insert(f.name.clone(), f.callees.clone());
            }
        }

        files.push(ProjectFile {
            is_frozen: is_uri_frozen(&file_uri),
            uri: file_uri,
            source,
            function_callees,
        });
    }

    Ok(ProjectAst {
        out_path,
        files,
    })
}

