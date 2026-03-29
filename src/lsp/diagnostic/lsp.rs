use crate::lsp::location::Location;
use crate::lsp::range::Range;
use crate::lsp::text_document::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::collections::HashMap;

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#documentDiagnosticParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDiagnosticParams {
    pub text_document: TextDocumentIdentifier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_result_id: Option<String>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#documentDiagnosticReport
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[allow(dead_code)]
pub enum DocumentDiagnosticReport {
    #[serde(rename_all = "camelCase")]
    #[serde(rename = "full")]
    Full {
        #[serde(skip_serializing_if = "Option::is_none")]
        result_id: Option<String>,
        items: Vec<Diagnostic>,

        #[serde(skip_serializing_if = "Option::is_none")]
        related_documents: Option<HashMap<String, UnattachedDocumentDiagnosticReport>>,
    },

    #[serde(rename_all = "camelCase")]
    #[serde(rename = "unchanged")]
    Unchanged {
        result_id: String,

        #[serde(skip_serializing_if = "Option::is_none")]
        related_documents: Option<HashMap<String, UnattachedDocumentDiagnosticReport>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
pub enum UnattachedDocumentDiagnosticReport {
    Full {
        kind: FullKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        result_id: Option<String>,
        items: Vec<Diagnostic>,
    },
    Unchanged {
        kind: UnchangedKind,
        result_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum FullKind {
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum UnchangedKind {
    Unchanged,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#diagnostic
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub range: Range,
    pub message: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<DiagnosticSeverity>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<DiagnosticCode>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_description: Option<CodeDescription>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<DiagnosticTag>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_information: Option<Vec<DiagnosticRelatedInformation>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#diagnosticSeverity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#diagnosticTag
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum DiagnosticTag {
    Unnecessary = 1,
    Deprecated = 2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DiagnosticCode {
    Int(i64),
    String(String),
}

impl DiagnosticCode {
    /// Check if this code is a specific string value.
    pub fn is_str(&self, s: &str) -> bool {
        matches!(self, DiagnosticCode::String(v) if v == s)
    }
}

impl Diagnostic {
    /// Check if the diagnostic has a specific string code.
    pub fn has_code(&self, s: &str) -> bool {
        self.code.as_ref().map_or(false, |c| c.is_str(s))
    }

    /// Create a new diagnostic with the standard source and a string code.
    pub fn new(source: &str, code: &str) -> Self {
        Self {
            source: Some(source.into()),
            code: Some(DiagnosticCode::String(code.into())),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeDescription {
    pub href: String, // URI
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticRelatedInformation {
    pub location: Location,
    pub message: String,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#diagnosticOptions
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticOptions {
    /// Whether the language has inter-file dependencies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inter_file_dependencies: Option<bool>,
    /// Whether the server provides workspace-wide diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_diagnostics: Option<bool>,
}
