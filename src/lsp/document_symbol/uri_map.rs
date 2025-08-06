use crate::lsp::document_symbol::lsp::DocumentSymbol;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use url::Url;

pub static URI_MAP: Lazy<DashMap<Url, Vec<DocumentSymbol>>> = Lazy::new(|| DashMap::new());
