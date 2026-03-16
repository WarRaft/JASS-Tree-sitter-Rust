use crate::lsp::location::Location;
use crate::lsp::range::Range;
use crate::lsp::text_document::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
pub enum FullKind {
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    #[serde(rename = "1")]
    Error = 1,

    #[serde(rename = "2")]
    Warning = 2,

    #[serde(rename = "3")]
    Information = 3,

    #[serde(rename = "4")]
    Hint = 4,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#diagnosticTag
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DiagnosticTag {
    Unnecessary = 1,
    Deprecated = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DiagnosticCode {
    Int(i64),
    String(String),
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
