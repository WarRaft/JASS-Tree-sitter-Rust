//! Builder subprocess management.
//!
//! Spawns and manages a separate builder process that:
//! - Runs in a separate task to avoid blocking the main parse loop
//! - Takes parse diagnostics and preserves them
//! - Generates additional multi-file diagnostics (unused functions, type mismatches, etc.)
//! - Has its own cancellation token
//! - Can be killed if a new parse/build cycle starts
//!
//! # Architecture: Builder as multi-file diagnostic source
//!
//! The builder **augments** diagnostics from the parse phase:
//!
//! 1. **Parse phase**: Generates syntax and local diagnostics (source="jass")
//! 2. **Builder phase**: Preserves parse diagnostics AND adds multi-file diagnostics
//! 3. **Builder adds**: Unused variables/functions (cross-file), type mismatches, etc. (source="build")
//! 4. **PARSE_CACHE merged**: Contains both parse AND build diagnostics
//!
//! This way we keep accurate syntax errors and local hints, while adding
//! cross-file analysis that requires whole-project knowledge.

use std::collections::HashMap;
use std::sync::Arc;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use tokio_util::sync::CancellationToken;
use url::Url;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::http::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
use crate::http::ref_map::{DeclKey, RefMap};
use crate::http::range::Range;
use crate::util::import_graph::IMPORT_GRAPH;

// ─── Global builder process registry ──────────────────────────────────────────

/// Per-URI builder process state.
///
/// Stores the cancellation token of the current running builder task.
/// When a new build starts for the same URI, the old token is cancelled.
pub struct BuilderProcessState {
    /// The cancellation token for the in-flight builder task.
    pub cancel_token: CancellationToken,
    /// Whether a build is currently in progress.
    pub is_running: AtomicBool,
}

impl BuilderProcessState {
    pub fn new() -> Self {
        Self {
            cancel_token: CancellationToken::new(),
            is_running: AtomicBool::new(false),
        }
    }
}

/// Per-entry-URI builder process state.
///
/// When parsing a file, we look for its entry point. The builder process
/// is keyed by entry URI, not the individual file URI, so that all files
/// in an import tree share the same builder.
pub static BUILDER_PROCESSES: Lazy<DashMap<Url, Arc<BuilderProcessState>>> =
    Lazy::new(DashMap::new);

/// Results collected by the builder for one file.
///
/// These get merged into `PARSE_CACHE` once the builder finishes.
#[derive(Debug, Clone)]
pub struct BuilderDiagnostics {
    /// URI of the file these diagnostics apply to
    pub uri: Url,
    /// List of diagnostics with source="build"
    pub diagnostics: Vec<Diagnostic>,
}

/// Get or create the builder process state for an entry URI.
pub fn get_or_create_builder_state(entry_uri: &Url) -> Arc<BuilderProcessState> {
    BUILDER_PROCESSES
        .entry(entry_uri.clone())
        .or_insert_with(|| Arc::new(BuilderProcessState::new()))
        .clone()
}

/// Cancel the in-flight builder process for an entry URI (if any).
///
/// Called when a new parse starts for a file in that import tree.
pub fn cancel_builder_for_entry(entry_uri: &Url) {
    // Avoid re-entrant map access: never insert while holding a `get()` guard.
    let should_replace = if let Some(state) = BUILDER_PROCESSES.get(entry_uri) {
        state.cancel_token.cancel();
        true
    } else {
        false
    };

    if should_replace {
        // Spawn a new token for the next build.
        let new_state = Arc::new(BuilderProcessState::new());
        BUILDER_PROCESSES.insert(entry_uri.clone(), new_state);
    }
}

fn builder_roots_for_uri(uri: &Url) -> Vec<Url> {
    let mut roots: Vec<Url> = IMPORT_GRAPH.cached_entry_points_for(uri).into_iter().collect();
    // Always include the current URI because parse fallback may key builder
    // tasks by the file itself when no stable entry root is available yet.
    roots.push(uri.clone());
    roots.sort_by(|a, b| a.path().cmp(b.path()));
    roots.dedup();
    roots
}

/// Wait a short time for builder tasks that affect `uri` to finish.
///
/// This is used by the document-update response path so newly merged
/// builder diagnostics can be included without requiring an extra edit.
pub async fn wait_builders_for_uri(uri: &Url, timeout_ms: u64) {
    use std::time::Duration;

    let roots = builder_roots_for_uri(uri);
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        let mut any_running = false;
        for root in &roots {
            if let Some(state) = BUILDER_PROCESSES.get(root) {
                if state.is_running.load(Ordering::Acquire) {
                    any_running = true;
                    break;
                }
            }
        }

        if !any_running || tokio::time::Instant::now() >= deadline {
            break;
        }

        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Spawn the builder process for an entry URI.
///
/// This runs on a spawned task, not the main parse thread, so it doesn't
/// block the parse loop. It collects multi-file diagnostics and stores them
/// in `PARSE_CACHE` with `source: "build"`.
///
/// # Arguments
///
/// * `entry_uri` — the entry file URI that triggered the build
/// * `config` — build configuration (mode, options, etc.)
///
/// The builder will:
/// 1. Collect the file order from the entry
/// 2. Read all files from disk/memory
/// 3. Build a shared AST
/// 4. Run diagnostics in the builder's own cancellation context
/// 5. Merge results into `PARSE_CACHE`
pub fn spawn_builder_task(entry_uri: &Url, config: BuilderConfig) {
    let entry_uri = entry_uri.clone();
    let state = get_or_create_builder_state(&entry_uri);
    let cancel_token = state.cancel_token.clone();

    state.is_running.store(true, Ordering::Release);

    tokio::spawn(async move {
        // Set the running flag
        defer_runner(|| {
            state.is_running.store(false, Ordering::Release);
        });

        // Run the builder in its own context with cancellation
        if let Err(e) = run_builder(&entry_uri, &config, &cancel_token).await {
            log::error!("builder: {}", e);
        }
    });
}

fn defer_runner<F: FnOnce()>(f: F) -> impl Drop {
    struct DeferGuard<F: FnOnce()> {
        f: Option<F>,
    }
    impl<F: FnOnce()> Drop for DeferGuard<F> {
        fn drop(&mut self) {
            if let Some(f) = self.f.take() {
                f()
            }
        }
    }
    DeferGuard { f: Some(f) }
}

// ─── Builder configuration ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BuilderConfig {
    /// Build mode: diagnostics-only or full build
    pub mode: crate::lng::jass::builder::PipelineMode,
    /// Build options (uglify, nolocal, etc.)
    pub opts: std::collections::HashSet<String>,
}

// ─── Builder implementation ───────────────────────────────────────────────────

/// Core builder task that runs in a separate tokio task.
///
/// This is where all the heavy lifting happens. It:
/// 1. Collects file order from the entry
/// 2. Reads all files
/// 3. Builds a master AST
/// 4. Runs diagnostics / transformations
/// 5. Collects results and merges them into PARSE_CACHE
async fn run_builder(
    entry_uri: &Url,
    config: &BuilderConfig,
    cancel_token: &CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Keep config wired into the builder task API even in diagnostics mode.
    let _ = (&config.mode, &config.opts);

    // Ensure the task can be cancelled
    tokio::select! {
        _ = cancel_token.cancelled() => {
            log::debug!("builder: cancelled for {}", entry_uri.path());
            return Ok(());
        }
        result = async {
            _run_builder_impl(entry_uri, config, cancel_token).await
        } => {
            result
        }
    }
}

async fn _run_builder_impl(
    entry_uri: &Url,
    _config: &BuilderConfig,
    cancel_token: &CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use crate::lng::jass::builder::collect;
    use crate::util::parse_cache::PARSE_CACHE;

    // Step 1: Collect file order (dependencies first, entry last)
    let file_order = collect::collect_file_order(entry_uri);
    if file_order.is_empty() {
        return Err("no files to build".into());
    }

    // Step 2: Read all files and build unified AST
    let mut file_sources = Vec::new();
    let mut missing_sources = Vec::new();
    for uri in &file_order {
        if cancel_token.is_cancelled() {
            log::debug!("builder: cancelled during source collection for {}", entry_uri.path());
            return Ok(());
        }
        if let Some(source) = collect::read_source(uri) {
            file_sources.push((uri.clone(), source));
        } else {
            log::warn!("builder: could not read {}", uri.path());
            missing_sources.push(uri.clone());
        }
    }

    // Step 3: Run diagnostics

    // Collect diagnostics per file.
    let mut result_map: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
    for uri in &file_order {
        result_map.entry(uri.clone()).or_default();
    }

    for uri in &missing_sources {
        result_map
            .entry(entry_uri.clone())
            .or_default()
            .push(Diagnostic {
                range: Range::default(),
                message: format!("builder: cannot read source for {}", uri.path()),
                severity: Some(DiagnosticSeverity::Warning),
                source: Some("build".to_string()),
                code: Some(DiagnosticCode::String("build-read-failed".to_string())),
                ..Default::default()
            });
    }

    // ── Step 3a: Collect all project-wide function names (exact-case).
    let mut function_names: HashMap<String, String> = HashMap::new();
    for uri in &file_order {
        if let Some(snap) = PARSE_CACHE.get(uri).map(|s| Arc::clone(s.value())) {
            for f in &snap.file_symbols.functions {
                function_names
                    .entry(f.name.clone())
                    .or_insert_with(|| f.name.clone());
            }
        }
    }

    // ── Step 3b: Detect variable/argument declarations that collide with function names.
    // Rule: no variable/argument name may equal any function name in the entry tree.
    for uri in &file_order {
        if cancel_token.is_cancelled() {
            log::debug!(
                "builder: cancelled during function-name collision check for {}",
                entry_uri.path()
            );
            return Ok(());
        }
        let Some(snapshot) = PARSE_CACHE.get(uri).map(|s| Arc::clone(s.value())) else {
            continue;
        };

        let collisions = collect_function_name_collision_diagnostics(
            &snapshot.ref_map,
            &snapshot.var_decl_keys,
            &snapshot.arg_decl_keys,
            &function_names,
        );
        result_map.entry(uri.clone()).or_default().extend(collisions);
    }

    // Project-wide function diagnostics: distribute to each file in the tree.
    for uri in &file_order {
        if cancel_token.is_cancelled() {
            log::debug!("builder: cancelled during diagnostics for {}", entry_uri.path());
            return Ok(());
        }

        // Clone snapshot Arc and drop DashMap guard immediately to avoid
        // lock-ordering deadlocks with nested graph/cache reads.
        let Some(snapshot) = PARSE_CACHE.get(uri).map(|s| Arc::clone(s.value())) else {
            continue;
        };

        let file_unused_suppressed = snapshot.file_symbols.file_ignore_tags.contains("unused");
        let file_cycle_suppressed = snapshot.file_symbols.file_ignore_tags.contains("cycle");
        let diag = crate::util::call_graph::diagnose_functions(uri);

        for name in &diag.unused {
            if file_unused_suppressed {
                continue;
            }
            let per_decl_suppressed = snapshot
                .file_symbols
                .functions
                .iter()
                .any(|f| f.name == *name && f.ignore_tags.contains("unused"));
            if per_decl_suppressed {
                continue;
            }
            if let Some(range) = find_function_decl_range(&snapshot, name) {
                result_map
                    .entry(uri.clone())
                    .or_default()
                    .push(Diagnostic {
                        range,
                        message: crate::util::i18n::unused_function(name),
                        severity: Some(DiagnosticSeverity::Hint),
                        source: Some("build".to_string()),
                        code: Some(DiagnosticCode::String("unused-function-project".to_string())),
                        ..Default::default()
                    });
            }
        }

        for name in &diag.in_cycle {
            if file_cycle_suppressed {
                continue;
            }
            let per_decl_suppressed = snapshot
                .file_symbols
                .functions
                .iter()
                .any(|f| f.name == *name && f.ignore_tags.contains("cycle"));
            if per_decl_suppressed {
                continue;
            }
            if let Some(range) = find_function_decl_range(&snapshot, name) {
                result_map
                    .entry(uri.clone())
                    .or_default()
                    .push(Diagnostic {
                        range,
                        message: crate::util::i18n::cyclic_call_chain(name),
                        severity: Some(DiagnosticSeverity::Warning),
                        source: Some("build".to_string()),
                        code: Some(DiagnosticCode::String("cyclic-call-project".to_string())),
                        ..Default::default()
                    });
            }
        }
    }

    let mut results = Vec::new();
    for (uri, diagnostics) in result_map {
        results.push(BuilderDiagnostics { uri, diagnostics });
    }

    // Step 4: Merge diagnostics with builder augmentation
    //
    // The builder preserves parse diagnostics (syntax, local errors) and adds
    // its own cross-file diagnostics. Both are kept in PARSE_CACHE.
    //
    // Strategy:
    // 1. Keep all parse diagnostics (source="jass" or None)
    // 2. Remove any old "build" source diagnostics (in case builder re-ran)
    // 3. Add new "build" source diagnostics from the builder
    for result in results {
        // Clone snapshot Arc first, then drop guard before insert().
        if let Some(old_snap) = PARSE_CACHE.get(&result.uri).map(|s| Arc::clone(s.value())) {
            let mut merged_diags = old_snap.diagnostics.clone();

            // Remove old "build" diagnostics (from previous builder run)
            merged_diags.retain(|d| d.source.as_deref() != Some("build"));

            // Add new "build" diagnostics
            merged_diags.extend(result.diagnostics);

            // Create a new snapshot with merged diagnostics
            let new_snap = Arc::new(crate::util::parse_cache::ParseSnapshot {
                folding: old_snap.folding.clone(),
                symbols: old_snap.symbols.clone(),
                semantic: std::sync::RwLock::new(Default::default()),
                diagnostics: merged_diags,
                links: old_snap.links.clone(),
                ref_map: old_snap.ref_map.clone(),
                file_symbols: old_snap.file_symbols.clone(),
                _type_map: old_snap._type_map.clone(),
                type_hints: old_snap.type_hints.clone(),
                ujapi_hints: old_snap.ujapi_hints.clone(),
                func_decl_keys: old_snap.func_decl_keys.clone(),
                var_decl_keys: old_snap.var_decl_keys.clone(),
                arg_decl_keys: old_snap.arg_decl_keys.clone(),
                colors: old_snap.colors.clone(),
            });

            PARSE_CACHE.insert(result.uri, new_snap);
        }
    }

    Ok(())
}

fn find_function_decl_range(
    snapshot: &crate::util::parse_cache::ParseSnapshot,
    name: &str,
) -> Option<Range> {
    for (&decl_key, group) in &snapshot.ref_map.groups {
        if !snapshot.func_decl_keys.contains(&decl_key) || group.name != name {
            continue;
        }
        if let Some(occ) = group.occurrences.iter().find(|o| o.is_decl) {
            return Some(occ.range.clone());
        }
    }
    None
}

fn collect_function_name_collision_diagnostics(
    ref_map: &RefMap,
    var_decl_keys: &std::collections::HashSet<DeclKey>,
    arg_decl_keys: &std::collections::HashSet<DeclKey>,
    function_names: &HashMap<String, String>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (&decl_key, group) in &ref_map.groups {
        if !var_decl_keys.contains(&decl_key) && !arg_decl_keys.contains(&decl_key) {
            continue;
        }
        let Some(func_name) = function_names.get(&group.name) else {
            continue;
        };
        if let Some(occ) = group.occurrences.iter().find(|o| o.is_decl) {
            out.push(Diagnostic {
                range: occ.range.clone(),
                message: crate::util::i18n::name_collides_with_function(&group.name, func_name),
                severity: Some(DiagnosticSeverity::Warning),
                source: Some("build".to_string()),
                code: Some(DiagnosticCode::String("name-collision-function-project".to_string())),
                ..Default::default()
            });
        }
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::highlight::DocumentHighlightKind;
    use crate::http::position::Position;
    use crate::http::ref_map::{Occurrence, RefGroup};
    use std::collections::HashSet;

    #[test]
    fn test_builder_state_creation() {
        let uri = Url::parse("file:///test.j").unwrap();
        let state = get_or_create_builder_state(&uri);
        assert!(!state.is_running.load(Ordering::Acquire));
    }

    #[test]
    fn test_function_name_collision_for_local_var() {
        let mut ref_map = RefMap::default();
        ref_map.groups.insert(
            7,
            RefGroup {
                name: "Cunt_ret".to_string(),
                occurrences: vec![Occurrence {
                    range: Range {
                        start: Position { line: 1, character: 12 },
                        end: Position { line: 1, character: 20 },
                    },
                    kind: DocumentHighlightKind::Write,
                    is_decl: true,
                }],
            },
        );

        let var_decl_keys = HashSet::from([7]);
        let arg_decl_keys = HashSet::new();
        let function_names = HashMap::from([("Cunt_ret".to_string(), "Cunt_ret".to_string())]);

        let out = collect_function_name_collision_diagnostics(
            &ref_map,
            &var_decl_keys,
            &arg_decl_keys,
            &function_names,
        );

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].code, Some(DiagnosticCode::String("name-collision-function-project".to_string())));
    }

    #[test]
    fn test_function_name_collision_is_case_sensitive() {
        let mut ref_map = RefMap::default();
        ref_map.groups.insert(
            7,
            RefGroup {
                name: "cunt_ret".to_string(),
                occurrences: vec![Occurrence {
                    range: Range {
                        start: Position { line: 1, character: 12 },
                        end: Position { line: 1, character: 20 },
                    },
                    kind: DocumentHighlightKind::Write,
                    is_decl: true,
                }],
            },
        );

        let var_decl_keys = HashSet::from([7]);
        let arg_decl_keys = HashSet::new();
        let function_names = HashMap::from([("Cunt_ret".to_string(), "Cunt_ret".to_string())]);

        let out = collect_function_name_collision_diagnostics(
            &ref_map,
            &var_decl_keys,
            &arg_decl_keys,
            &function_names,
        );

        assert!(out.is_empty());
    }
}

