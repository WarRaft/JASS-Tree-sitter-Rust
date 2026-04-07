pub(crate) mod compute;

use crate::http::diagnostic::Diagnostic;
use crate::http::rename::WorkspaceEdit;
use crate::http::range::Range;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

// ─── Request ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CodeActionParams {
    pub uri: Url,
    pub range: Range,
    pub context: CodeActionContext,
}

#[derive(Debug, Deserialize)]
pub struct CodeActionContext {
    pub diagnostics: Vec<Diagnostic>,

    #[serde(default)]
    pub only: Option<Vec<String>>,
}

// ─── Response ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeAction {
    pub title: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Vec<Diagnostic>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<WorkspaceEdit>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Command>,
}

#[derive(Debug, Serialize)]
pub struct Command {
    pub title: String,
    pub command: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<Value>>,
}

/// Code Action kind constants.
pub const CODE_ACTION_KIND_QUICKFIX: &str = "quickfix";
pub const CODE_ACTION_KIND_REFACTOR: &str = "refactor";

