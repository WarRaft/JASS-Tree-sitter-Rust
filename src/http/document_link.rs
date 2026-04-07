use crate::http::range::Range;

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#documentLink
#[derive(Debug, Clone)]
pub struct DocumentLink {
    /// The range this link applies to.
    pub range: Range,

    /// The uri this link points to.
    pub target: Option<String>,

    /// An optional tooltip shown when hovering over the link.
    pub tooltip: Option<String>,
}

