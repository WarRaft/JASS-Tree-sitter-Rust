//! Shared `//import` and `//set` comment-directive logic used by both JASS and AngelScript.
//!
//! Both languages use identical comment-based directives at the top of a file:
//!
//! ```text
//! //import path/to/file
//! //import! frozen/file
//! //set key value
//! ```
//!
//! This module provides the directive data structures, the comment→directive
//! rewriting logic, and semantic-token helpers so that each language only
//! needs a thin adapter.

use crate::lsp::diagnostic::lsp::{Diagnostic, DiagnosticSeverity};
use crate::http::semantic::hub::Hub;
use crate::http::semantic::token::Kind as TokenKind;
use crate::util::roper::node::NodeExt;
use lapce_xi_rope::Rope;
use tree_sitter::Node;
use std::collections::{HashMap, HashSet};

// ─── Setting type system ─────────────────────────────────────────────────────

/// The kind of value a `//set` key expects.
///
/// Used for validation, completion, hover docs, and semantic coloring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetValueKind {
    /// `1` or `0` — a boolean toggle.
    Bool,
    /// A file or directory path (enables path completion).
    Path,
    /// A shell command to execute (free-form text).
    Command,
    /// Space-separated tags from a fixed set of allowed values.
    Tags(&'static [&'static str]),
}

/// Descriptor for a single `//set` key.
///
/// All known keys are registered in [`SET_DEFS`].  Unknown keys are silently
/// accepted for forward-compatibility but don't receive validation or
/// completion.
#[derive(Debug, Clone)]
pub struct SetDef {
    /// The key name (e.g. `"hint"`).
    pub key: &'static str,
    /// Value type.
    pub kind: SetValueKind,
    /// Default value (shown in docs and used when the user types the key
    /// without a value in the completion snippet).
    pub default: &'static str,
    /// Sort order in the completion list (lower = higher).
    pub sort_order: u8,
}

/// All known `//set` keys.
///
/// To add a new setting:
/// 1. Append a `SetDef` here.
/// 2. Consume it in the appropriate handler (inlay hints, build, etc.).
/// 3. Update `docs/jass/set/*.md`.
pub static SET_DEFS: &[SetDef] = &[
    SetDef {
        key: "hint",
        kind: SetValueKind::Tags(&["ref", "type"]),
        default: "",
        sort_order: 0,
    },
    SetDef {
        key: "build-jass",
        kind: SetValueKind::Path,
        default: "./",
        sort_order: 1,
    },
    SetDef {
        key: "build-as",
        kind: SetValueKind::Path,
        default: "./",
        sort_order: 2,
    },
    SetDef {
        key: "backup",
        kind: SetValueKind::Path,
        default: "./",
        sort_order: 3,
    },
    SetDef {
        key: "build-uglify",
        kind: SetValueKind::Bool,
        default: "0",
        sort_order: 4,
    },
    SetDef {
        key: "build-before",
        kind: SetValueKind::Command,
        default: "",
        sort_order: 5,
    },
    SetDef {
        key: "build-after",
        kind: SetValueKind::Command,
        default: "",
        sort_order: 6,
    },
];

/// Look up a `SetDef` by key name.
pub fn find_set_def(key: &str) -> Option<&'static SetDef> {
    SET_DEFS.iter().find(|d| d.key == key)
}

// ─── Template variables for Command values ───────────────────────────────────

/// Descriptor for a `{{variable}}` template placeholder usable inside
/// `build-before` / `build-after` command values.
#[derive(Debug, Clone)]
pub struct TemplateVar {
    /// Variable name (without the `{{ }}` delimiters).
    pub name: &'static str,
}

/// All known `{{variable}}` names.
///
/// To add a new template variable:
/// 1. Append a `TemplateVar` here.
/// 2. Expand it in `run_build_hook` (`build/mod.rs`).
/// 3. Add an i18n description via `template_var_detail`.
pub static TEMPLATE_VARS: &[TemplateVar] = &[
    TemplateVar { name: "entry" },
    TemplateVar { name: "entry-dir" },
    TemplateVar { name: "target-jass" },
    TemplateVar { name: "target-as" },
];

/// Look up a `TemplateVar` by name.
pub fn find_template_var(name: &str) -> Option<&'static TemplateVar> {
    TEMPLATE_VARS.iter().find(|v| v.name == name)
}

/// Find all `{{variable}}` occurrences in a string.
///
/// Returns `(byte_offset_of_open_brace, full_match_len, var_name)` tuples.
pub fn find_template_spans(text: &str) -> Vec<(usize, usize, String)> {
    let mut result = Vec::new();
    let mut search_from = 0;
    while let Some(open) = text[search_from..].find("{{") {
        let abs_open = search_from + open;
        if let Some(close) = text[abs_open + 2..].find("}}") {
            let name = &text[abs_open + 2..abs_open + 2 + close];
            // Only accept names without nested braces or whitespace
            if !name.contains('{') && !name.contains('}') && !name.contains(' ') && !name.contains('\t') && !name.is_empty() {
                let full_len = 2 + close + 2; // {{ + name + }}
                result.push((abs_open, full_len, name.to_string()));
                search_from = abs_open + full_len;
            } else {
                search_from = abs_open + 2;
            }
        } else {
            break;
        }
    }
    result
}

/// Validate a value against the expected `SetValueKind`.
///
/// Returns `None` if the value is valid, or `Some(message)` with an
/// error description.
pub fn validate_set_value(def: &SetDef, value: &str) -> Option<String> {
    match def.kind {
        SetValueKind::Bool => {
            if value != "0" && value != "1" {
                Some(crate::util::i18n::invalid_bool_value(value, def.key))
            } else {
                None
            }
        }
        SetValueKind::Path => {
            // Paths are free-form; only empty is caught by the generic
            // "missing value" diagnostic.
            None
        }
        SetValueKind::Command => {
            // Commands are free-form shell strings; no validation.
            None
        }
        SetValueKind::Tags(allowed) => {
            for word in value.split_whitespace() {
                if !allowed.contains(&word) {
                    return Some(crate::util::i18n::unknown_tag_value(word, def.key, allowed));
                }
            }
            None
        }
    }
}

// ─── Directive data ──────────────────────────────────────────────────────────

/// `//import path/to/file` or `//import! path/to/file`
///
/// Recognized only at the root level, before the first language statement.
/// The `//import` prefix must start at column 0 with no space after `//`.
///
/// `//import!` marks the target as **frozen** — a read-only file that we
/// cannot modify; we only pull declarations from it.
#[derive(Debug, Clone)]
pub struct ImportDirective<'tree> {
    /// The original comment CST node.
    pub node: Node<'tree>,
    /// `true` when the directive is `//import!` — the imported file is
    /// **frozen** (read-only): we only pull declarations from it without
    /// allowing edits.
    pub frozen: bool,
    /// The raw relative path string (everything after `//import ` or `//import! `).
    pub path: String,
}

/// `//set <key> <value>` — file-local configuration directive.
///
/// Recognized only at the root level, before the first language statement
/// (alongside `//import` directives).  The `//set` prefix must start at
/// column 0 with no space after `//`.
#[derive(Debug, Clone)]
pub struct SetDirective<'tree> {
    /// The original comment CST node.
    pub node: Node<'tree>,
    /// The setting key (e.g. `hint`).
    pub key: String,
    /// The raw value string (everything after the key until end-of-line, trimmed).
    pub value: String,
}

/// `//ignore <tag…>` — file-level diagnostic suppression directive.
///
/// Recognized only at the root level, before the first language statement
/// (alongside `//import` and `//set`).  The `//ignore` prefix must start at
/// column 0 with no space after `//`.
///
/// Multiple tags can be listed on one line: `//ignore unused leak`.
#[derive(Debug, Clone)]
pub struct IgnoreDirective<'tree> {
    /// The original comment CST node.
    pub node: Node<'tree>,
    /// Suppression tags (e.g. `["unused", "leak"]`).
    pub tags: Vec<String>,
}

/// `//import-ujapi! <path>` — download & frozen-import from UjAPI.
///
/// Fetches `uJAPIFiles/common.j` from the latest UjAPI GitHub release and
/// saves it to the user-specified `<path>` (relative to the current file).
/// The imported file is treated as **frozen** (read-only), identical to
/// `//import!`.
#[derive(Debug, Clone)]
pub struct UjapiDirective<'tree> {
    /// The original comment CST node.
    pub node: Node<'tree>,
    /// The raw relative destination path.
    pub path: String,
}

/// `//entry` — marks the file as a build entry point.
///
/// Recognized only at the root level, before the first language statement
/// (alongside `//import`, `//set`, and `//ignore`).  The `//entry` prefix
/// must start at column 0 with no space after `//`.
///
/// Entry points define the roots of the dependency tree for builds and
/// tree-shaking.  Only files transitively reachable from an entry point
/// are included in the build output, and only functions reachable from
/// entry-point files' `main`/`config` are considered live.
#[derive(Debug, Clone)]
pub struct EntryDirective<'tree> {
    /// The original comment CST node.
    pub node: Node<'tree>,
}

// ─── Known ignore tags ──────────────────────────────────────────────────────

/// Descriptor for a known `//ignore` / `//@ignore` tag.
#[derive(Debug, Clone)]
pub struct IgnoreTagDef {
    /// Tag name (e.g. `"unused"`).
    pub tag: &'static str,
}

/// All recognized suppression tags for `//ignore` and `//@ignore`.
pub static IGNORE_TAGS: &[IgnoreTagDef] = &[
    IgnoreTagDef { tag: "unused" },
    IgnoreTagDef { tag: "leak" },
    IgnoreTagDef { tag: "cycle" },
];

/// Look up an `IgnoreTagDef` by name.
pub fn find_ignore_tag(tag: &str) -> Option<&'static IgnoreTagDef> {
    IGNORE_TAGS.iter().find(|d| d.tag == tag)
}

// ─── Directive enum (language-agnostic) ──────────────────────────────────────

/// Result of trying to parse a leading comment as a directive.
#[derive(Debug, Clone)]
pub enum Directive<'tree> {
    Import(ImportDirective<'tree>),
    Set(SetDirective<'tree>),
    Ignore(IgnoreDirective<'tree>),
    Ujapi(UjapiDirective<'tree>),
    Entry(EntryDirective<'tree>),
}

/// Try to parse a comment CST node as a directive.
///
/// Returns `Some(Directive)` if the node is a line comment at column 0 that
/// matches `//import`, `//import!`, `//set`, or `//ignore`.
pub fn try_parse_directive<'tree>(node: &Node<'tree>, src: &[u8]) -> Option<Directive<'tree>> {
    if node.start_position().column != 0 {
        return None;
    }

    let text = std::str::from_utf8(&src[node.start_byte()..node.end_byte()]).ok()?;

    // ── //import-ujapi! ───────────────────────────────────────────
    if let Some(rest) = text.strip_prefix("//import-ujapi!") {
        let path = rest.trim().to_string();
        return Some(Directive::Ujapi(UjapiDirective {
            node: *node,
            path,
        }));
    }
    // ── //import! ────────────────────────────────────────────────
    if let Some(rest) = text.strip_prefix("//import!") {
        let path = rest.trim().to_string();
        return Some(Directive::Import(ImportDirective {
            node: *node,
            frozen: true,
            path,
        }));
    }
    // ── //import ─────────────────────────────────────────────────
    if let Some(rest) = text.strip_prefix("//import") {
        if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
            let path = rest.trim().to_string();
            return Some(Directive::Import(ImportDirective {
                node: *node,
                frozen: false,
                path,
            }));
        }
    }
    // ── //set ────────────────────────────────────────────────────
    if let Some(rest) = text.strip_prefix("//set") {
        if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
            let trimmed = rest.trim();
            let (key, value) = match trimmed.find(|c: char| c == ' ' || c == '\t') {
                Some(pos) => (
                    trimmed[..pos].to_string(),
                    trimmed[pos..].trim().to_string(),
                ),
                None => (trimmed.to_string(), String::new()),
            };
            return Some(Directive::Set(SetDirective {
                node: *node,
                key,
                value,
            }));
        }
    }
    // ── //ignore ─────────────────────────────────────────────────
    if let Some(rest) = text.strip_prefix("//ignore") {
        if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
            let tags: Vec<String> = rest.split_whitespace().map(|s| s.to_string()).collect();
            return Some(Directive::Ignore(IgnoreDirective {
                node: *node,
                tags,
            }));
        }
    }
    // ── //entry ──────────────────────────────────────────────────
    if text == "//entry" || text.strip_prefix("//entry").map_or(false, |r| r.starts_with(' ') || r.starts_with('\t')) {
        return Some(Directive::Entry(EntryDirective {
            node: *node,
        }));
    }

    None
}

// ─── Semantic token helpers ─────────────────────────────────────────────────

/// Emit semantic tokens for an `//import` or `//import!` directive.
///
/// Returns whether the path was empty (caller can add the node to
/// `directive_nodes` before calling this).
pub fn visit_import_semantic(
    imp: &ImportDirective,
    semantic: &mut Hub,
    diagnostics: &mut Vec<Diagnostic>,
    rope: &Rope,
) {
    let node = &imp.node;
    let prefix_len = if imp.frozen {
        "//import!".len()
    } else {
        "//import".len()
    };

    // Macro token for the "//import" / "//import!" prefix
    let start_byte = node.start_byte();
    semantic.add_range(start_byte, prefix_len, rope, TokenKind::Macro, 0u32);

    if imp.path.is_empty() {
        diagnostics.push(Diagnostic {
            range: node.to_range(rope),
            message: crate::util::i18n::missing_import_path().into(),
            severity: Some(DiagnosticSeverity::Error),
            ..Diagnostic::new("jass", "import-missing-path")
        });
    } else {
        // String token for the path (skip whitespace between prefix and path)
        let full_text_bytes = node.end_byte() - node.start_byte();
        if full_text_bytes > prefix_len {
            let path_offset = start_byte + prefix_len;
            let path_len = full_text_bytes - prefix_len;
            let raw = &rope.slice_to_cow(path_offset..path_offset + path_len);
            let trimmed = raw.trim_start();
            let ws_len = raw.len() - trimmed.len();
            if !trimmed.is_empty() {
                semantic.add_range(
                    path_offset + ws_len,
                    trimmed.len(),
                    rope,
                    TokenKind::String,
                    0u32,
                );
            }
        }
    }
}

/// Emit semantic tokens for a `//set key value` directive and collect the
/// setting into `file_settings`.
pub fn visit_set_semantic(
    sd: &SetDirective,
    semantic: &mut Hub,
    diagnostics: &mut Vec<Diagnostic>,
    file_settings: &mut HashMap<String, String>,
    rope: &Rope,
) {
    let node = &sd.node;
    let prefix_len = "//set".len();
    let start_byte = node.start_byte();

    // Macro token for the "//set" prefix
    semantic.add_range(start_byte, prefix_len, rope, TokenKind::Macro, 0u32);

    let full_text_bytes = node.end_byte() - node.start_byte();
    if full_text_bytes > prefix_len {
        let after_prefix_offset = start_byte + prefix_len;
        let after_prefix_len = full_text_bytes - prefix_len;
        let raw = rope
            .slice_to_cow(after_prefix_offset..after_prefix_offset + after_prefix_len)
            .to_string();
        let trimmed = raw.trim_start();
        let ws_before_key = raw.len() - trimmed.len();

        if !trimmed.is_empty() {
            // Key: Property token
            let key_len = trimmed
                .find(|c: char| c == ' ' || c == '\t')
                .unwrap_or(trimmed.len());
            let key_offset = after_prefix_offset + ws_before_key;
            semantic.add_range(key_offset, key_len, rope, TokenKind::Property, 0u32);

            // Value: token (type-aware coloring)
            if key_len < trimmed.len() {
                let after_key = &trimmed[key_len..];
                let value_part = after_key.trim_start();
                let ws_before_value = after_key.len() - value_part.len();
                if !value_part.is_empty() {
                    let val_offset = key_offset + key_len + ws_before_value;


                    let found_def = find_set_def(&sd.key);
                    let is_command = matches!(found_def, Some(def) if def.kind == SetValueKind::Command);
                    let is_tags = matches!(found_def, Some(def) if matches!(def.kind, SetValueKind::Tags(_)));

                    if is_tags {
                        // Color each tag word as EnumMember (like //ignore tags).
                        let mut cursor = 0usize;
                        for tag in value_part.split_whitespace() {
                            if let Some(pos) = value_part[cursor..].find(tag) {
                                let tag_offset = val_offset + cursor + pos;
                                semantic.add_range(tag_offset, tag.len(), rope, TokenKind::EnumMember, 0u32);
                                cursor += pos + tag.len();
                            }
                        }
                    } else if is_command {
                        // Split value into literal String parts and {{var}} Variable parts.
                        let spans = find_template_spans(value_part);
                        let mut cursor = 0usize;
                        for (span_off, span_len, var_name) in &spans {
                            // Literal text before this template var
                            if *span_off > cursor {
                                semantic.add_range(
                                    val_offset + cursor,
                                    span_off - cursor,
                                    rope,
                                    TokenKind::String,
                                    0u32,
                                );
                            }
                            // The {{ and }} delimiters as Operator
                            semantic.add_range(val_offset + span_off, 2, rope, TokenKind::Operator, 0u32);
                            // The variable name as Variable (or as Macro if unknown)
                            let name_offset = val_offset + span_off + 2;
                            let name_len = var_name.len();
                            let name_kind = if find_template_var(var_name).is_some() {
                                TokenKind::Variable
                            } else {
                                TokenKind::Macro
                            };
                            semantic.add_range(name_offset, name_len, rope, name_kind, 0u32);
                            // Closing }}
                            semantic.add_range(name_offset + name_len, 2, rope, TokenKind::Operator, 0u32);

                            // Diagnostic for unknown template variable
                            if find_template_var(var_name).is_none() {
                                diagnostics.push(Diagnostic {
                                    range: node.to_range(rope),
                                    message: crate::util::i18n::unknown_template_var(var_name),
                                    severity: Some(DiagnosticSeverity::Warning),
                                    ..Diagnostic::new("jass", "unknown-template-var")
                                });
                            }

                            cursor = span_off + span_len;
                        }
                        // Trailing literal text after the last template var
                        if cursor < value_part.len() {
                            semantic.add_range(
                                val_offset + cursor,
                                value_part.len() - cursor,
                                rope,
                                TokenKind::String,
                                0u32,
                            );
                        }
                    } else {
                        // Pick token kind based on value type: Bool → Number, Path → String
                        let val_token = match find_set_def(&sd.key) {
                            Some(def) if def.kind == SetValueKind::Bool => TokenKind::Number,
                            _ => TokenKind::String,
                        };
                        semantic.add_range(
                            val_offset,
                            value_part.len(),
                            rope,
                            val_token,
                            0u32,
                        );
                    }
                }
            }
        }
    }

    if sd.key.is_empty() {
        diagnostics.push(Diagnostic {
            range: node.to_range(rope),
            message: crate::util::i18n::missing_setting_key().into(),
            severity: Some(DiagnosticSeverity::Error),
            ..Diagnostic::new("jass", "setting-missing-key")
        });
    } else if sd.value.is_empty() {
        // Tags kind: empty value is valid (means "none enabled").
        let is_tags = matches!(find_set_def(&sd.key), Some(def) if matches!(def.kind, SetValueKind::Tags(_)));
        if !is_tags {
            diagnostics.push(Diagnostic {
                range: node.to_range(rope),
                message: crate::util::i18n::missing_setting_value(&sd.key),
                severity: Some(DiagnosticSeverity::Warning),
                ..Diagnostic::new("jass", "setting-missing-value")
            });
        }
        file_settings.insert(sd.key.clone(), sd.value.clone());
    } else {
        // Validate against the registry
        if let Some(def) = find_set_def(&sd.key) {
            if let Some(err_msg) = validate_set_value(def, &sd.value) {
                diagnostics.push(Diagnostic {
                    range: node.to_range(rope),
                    message: err_msg,
                    severity: Some(DiagnosticSeverity::Warning),
                    ..Diagnostic::new("jass", "setting-invalid")
                });
            }
        }
        file_settings.insert(sd.key.clone(), sd.value.clone());
    }
}

/// Emit semantic tokens for an `//ignore tag1 tag2` directive and collect
/// the tags into `file_ignore_tags`.
pub fn visit_ignore_semantic(
    ig: &IgnoreDirective,
    semantic: &mut Hub,
    diagnostics: &mut Vec<Diagnostic>,
    file_ignore_tags: &mut HashSet<String>,
    rope: &Rope,
) {
    let node = &ig.node;
    let prefix_len = "//ignore".len();
    let start_byte = node.start_byte();

    // Macro token for the "//ignore" prefix
    semantic.add_range(start_byte, prefix_len, rope, TokenKind::Macro, 0u32);

    // Color each tag as EnumMember
    let full_text_bytes = node.end_byte() - node.start_byte();
    if full_text_bytes > prefix_len {
        let after_offset = start_byte + prefix_len;
        let after_raw = rope
            .slice_to_cow(after_offset..start_byte + full_text_bytes)
            .to_string();
        let mut cursor = 0usize;
        for tag in after_raw.split_whitespace() {
            if let Some(pos) = after_raw[cursor..].find(tag) {
                let tag_offset = after_offset + cursor + pos;
                semantic.add_range(tag_offset, tag.len(), rope, TokenKind::EnumMember, 0u32);
                cursor += pos + tag.len();
            }
        }
    }

    if ig.tags.is_empty() {
        diagnostics.push(Diagnostic {
            range: node.to_range(rope),
            message: crate::util::i18n::missing_ignore_tag().into(),
            severity: Some(DiagnosticSeverity::Warning),
            ..Diagnostic::new("jass", "ignore-missing-tag")
        });
    } else {
        for tag in &ig.tags {
            file_ignore_tags.insert(tag.clone());
        }
    }
}

/// Process import directives and produce document links + diagnostics.
///
/// Shared between JASS and AS.  Each `ImportDirective` is resolved via
/// `resolve_import` and the results are pushed into the output vecs.
#[allow(dead_code)]
pub fn process_imports<'a>(
    uri: &url::Url,
    directives: impl Iterator<Item = &'a ImportDirective<'static>>,
    src: &[u8],
    rope: &Rope,
    imports: &mut HashSet<url::Url>,
    frozen_imports: &mut HashSet<url::Url>,
    links: &mut Vec<crate::lsp::document_link::lsp::DocumentLink>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use crate::lsp::position::Position;
    use crate::lsp::range::Range;
    use crate::util::import_graph::resolve_import;

    for imp in directives {
        if imp.path.is_empty() {
            continue;
        }

        let node = &imp.node;
        let prefix_len = if imp.frozen {
            "//import!".len()
        } else {
            "//import".len()
        };
        let node_text =
            std::str::from_utf8(&src[node.start_byte()..node.end_byte()]).unwrap_or("");
        let after_prefix = &node_text[prefix_len..];
        let ws_len = after_prefix.len() - after_prefix.trim_start().len();
        let path_start_byte = node.start_byte() + prefix_len + ws_len;
        let path_end_byte = node.start_byte() + prefix_len + ws_len + imp.path.len();

        let path_range = Range {
            start: Position::from_byte_offset(rope, path_start_byte).unwrap_or_default(),
            end: Position::from_byte_offset(rope, path_end_byte).unwrap_or_default(),
        };

        match resolve_import(uri, &imp.path) {
            Some(resolved) => {
                imports.insert(resolved.url.clone());
                if imp.frozen {
                    frozen_imports.insert(resolved.url.clone());
                }
                if resolved.exists {
                    links.push(crate::lsp::document_link::lsp::DocumentLink {
                        range: path_range,
                        target: Some(resolved.url.to_string()),
                        tooltip: Some(resolved.url.to_string()),
                    });
                } else {
                    diagnostics.push(Diagnostic {
                        range: path_range,
                        message: crate::util::i18n::file_not_found(&imp.path),
                        severity: Some(DiagnosticSeverity::Error),
                        ..Diagnostic::new("jass", "import-not-found")
                    });
                }
            }
            None => {
                diagnostics.push(Diagnostic {
                    range: path_range,
                    message: crate::util::i18n::cannot_resolve_import(&imp.path),
                    severity: Some(DiagnosticSeverity::Error),
                    ..Diagnostic::new("jass", "import-resolve")
                });
            }
        }
    }
}

/// Emit semantic tokens for an `//import-ujapi! path` directive.
pub fn visit_ujapi_semantic(
    ud: &UjapiDirective,
    semantic: &mut Hub,
    diagnostics: &mut Vec<Diagnostic>,
    rope: &Rope,
) {
    let node = &ud.node;
    let prefix_len = "//import-ujapi!".len();
    let start_byte = node.start_byte();

    // Macro token for the "//import-ujapi!" prefix
    semantic.add_range(start_byte, prefix_len, rope, TokenKind::Macro, 0u32);

    if ud.path.is_empty() {
        diagnostics.push(Diagnostic {
            range: node.to_range(rope),
            message: crate::util::i18n::ujapi_missing_path().into(),
            severity: Some(DiagnosticSeverity::Error),
            ..Diagnostic::new("jass", "ujapi-missing-path")
        });
    } else {
        // String token for the path
        let full_text_bytes = node.end_byte() - node.start_byte();
        if full_text_bytes > prefix_len {
            let path_offset = start_byte + prefix_len;
            let path_len = full_text_bytes - prefix_len;
            let raw = &rope.slice_to_cow(path_offset..path_offset + path_len);
            let trimmed = raw.trim_start();
            let ws_len = raw.len() - trimmed.len();
            if !trimmed.is_empty() {
                semantic.add_range(
                    path_offset + ws_len,
                    trimmed.len(),
                    rope,
                    TokenKind::String,
                    0u32,
                );
            }
        }
    }
}

/// Emit semantic tokens for an `//entry` directive.
pub fn visit_entry_semantic(
    ed: &EntryDirective,
    semantic: &mut Hub,
    rope: &Rope,
) {
    let node = &ed.node;
    let prefix_len = "//entry".len();
    let start_byte = node.start_byte();

    // Macro token for the "//entry" prefix
    semantic.add_range(start_byte, prefix_len, rope, TokenKind::Macro, 0u32);
}

/// Check whether a CST node's `start_byte` is in the set of directive nodes,
/// meaning it should be skipped during CST DFS semantic-token generation.
#[allow(dead_code)]
pub fn is_directive_node(start_byte: usize, directive_nodes: &HashSet<usize>) -> bool {
    directive_nodes.contains(&start_byte)
}

