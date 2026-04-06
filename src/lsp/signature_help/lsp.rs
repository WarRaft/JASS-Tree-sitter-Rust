use crate::lsp::hover::lsp::MarkupContent;
use crate::lsp::position::Position;
use crate::lsp::text_document::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#signatureHelpParams
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureHelpParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<SignatureHelpContext>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#signatureHelpContext
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureHelpContext {
    pub trigger_kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_character: Option<String>,
    pub is_retrigger: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_signature_help: Option<SignatureHelp>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#signatureHelp
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureHelp {
    pub signatures: Vec<SignatureInformation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_signature: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_parameter: Option<u32>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#signatureInformation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInformation {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<MarkupContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<ParameterInformation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_parameter: Option<u32>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#parameterInformation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterInformation {
    /// Label offsets [start, end] into the signature label string.
    pub label: [u32; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<MarkupContent>,
}

