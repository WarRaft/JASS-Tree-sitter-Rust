use crate::lsp::call_hierarchy::lsp::CallHierarchyOptions;
use crate::lsp::code_lens::lsp::CodeLensOptions;
use crate::lsp::completion::lsp::CompletionOptions;
use crate::lsp::document_link::lsp::DocumentLinkOptions;
use crate::lsp::document_symbol::lsp::DocumentSymbolOptions;
use crate::lsp::folding::lsp::FoldingRangeOptions;
use crate::lsp::formatting::lsp::DocumentFormattingOptions;
use crate::lsp::inlay_hint::lsp::InlayHintOptions;
use crate::lsp::rename::lsp::{RenameOptions, WorkspaceServerCapabilities};
use crate::lsp::semantic::lsp::SemanticTokensOptions;
use crate::lsp::signature_help::lsp::SignatureHelpOptions;
use crate::lsp::text_document::TextDocumentSyncOptions;
use crate::lsp::type_hierarchy::lsp::TypeHierarchyOptions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#initialize
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub process_id: Option<i64>,
    pub root_path: Option<String>,
    pub capabilities: Option<Value>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#initializeResult
#[derive(Debug, Serialize, Deserialize)]
pub struct InitializeResult {
    pub capabilities: ServerCapabilities,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#serverCapabilities
#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_document_sync: Option<TextDocumentSyncOptions>,
    pub semantic_tokens_provider: Option<SemanticTokensOptions>,
    pub document_symbol_provider: Option<DocumentSymbolOptions>,
    pub folding_range_provider: Option<FoldingRangeOptions>,
    pub completion_provider: Option<CompletionOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_provider: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_highlight_provider: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition_provider: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references_provider: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inlay_hint_provider: Option<InlayHintOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rename_provider: Option<RenameOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_action_provider: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_link_provider: Option<DocumentLinkOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_formatting_provider: Option<DocumentFormattingOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_provider: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceServerCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_help_provider: Option<SignatureHelpOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_lens_provider: Option<CodeLensOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_hierarchy_provider: Option<CallHierarchyOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_hierarchy_provider: Option<TypeHierarchyOptions>,
}
