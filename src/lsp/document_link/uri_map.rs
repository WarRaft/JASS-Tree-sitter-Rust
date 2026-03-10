use crate::lsp::document_link::lsp::DocumentLink;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use url::Url;

/// Per-document list of clickable links (import paths → file URIs).
pub static URI_MAP: Lazy<DashMap<Url, Vec<DocumentLink>>> = Lazy::new(DashMap::new);

