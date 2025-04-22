use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_repr::{Deserialize_repr, Serialize_repr};

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
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#semanticTokensOptions
#[derive(Debug, Serialize, Deserialize)]
pub struct SemanticTokensOptions {
    pub legend: SemanticTokensLegend,
    pub full: bool,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#semanticTokensLegend
/// https://code.visualstudio.com/api/language-extensions/semantic-highlight-guide
#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticTokensLegend {
    pub token_types: Vec<String>,
    pub token_modifiers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenType {
    Namespace,     // For identifiers that declare or reference a namespace, module, or package.
    Class,         // For identifiers that declare or reference a class type.
    Enum,          // For identifiers that declare or reference an enumeration type.
    Interface,     // For identifiers that declare or reference an interface type.
    Struct,        // For identifiers that declare or reference a struct type.
    TypeParameter, // For identifiers that declare or reference a type parameter.
    Type,          // For identifiers that declare or reference a type that is not covered above.
    Parameter,     // For identifiers that declare or reference a function or method parameters.
    Variable,      // For identifiers that declare or reference a local or global variable.
    Property, // For identifiers that declare or reference a member property, member field, or member variable.
    EnumMember, // For identifiers that declare or reference an enumeration property, constant, or member.
    Decorator,  // For identifiers that declare or reference decorators and annotations.
    Event,      // For identifiers that declare an event property.
    Function,   // For identifiers that declare a function.
    Method,     // For identifiers that declare a member function or method.
    Macro,      // For identifiers that declare a macro.
    Label,      // For identifiers that declare a label.
    Comment,    // For tokens that represent a comment.
    String,     // For tokens that represent a string literal.
    Keyword,    // For tokens that represent a language keyword.
    Number,     // For tokens that represent a number literal.
    Regexp,     // For tokens that represent a regular expression literal.
    Operator,   // For tokens that represent an operator.
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenModifier {
    Declaration,    //	For declarations of symbols.
    Definition,     //	For definitions of symbols, for example, in header files.
    Readonly,       //	For readonly variables and member fields (constants).
    Static,         //	For class members (static members).
    Deprecated,     //	For symbols that should no longer be used.
    Abstract,       //	For types and member functions that are abstract.
    Async,          //	For functions that are marked async.
    Modification,   //	For variable references where the variable is assigned to.
    Documentation,  //	For occurrences of symbols in documentation.
    DefaultLibrary, //	For symbols that are part of the standard library.
}

/*
export interface SemanticTokensOptions extends WorkDoneProgressOptions {
    /**
     * The legend used by the server
     */
    legend: SemanticTokensLegend;

    /**
     * Server supports providing semantic tokens for a specific range
     * of a document.
     */
    range?: boolean | {
    };

    /**
     * Server supports providing semantic tokens for a full document.
     */
    full?: boolean | {
        /**
         * The server supports deltas for full documents.
         */
        delta?: boolean;
    };
}
 */

/*

   semantic_tokens_provider?: SemanticTokensOptions
       | SemanticTokensRegistrationOptions;
*/

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocumentSyncOptions
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentSyncOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_close: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<TextDocumentSyncKind>,
}

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocumentSyncKind
#[derive(Debug, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum TextDocumentSyncKind {
    None = 0,
    Full = 1,
    Incremental = 2,
}
