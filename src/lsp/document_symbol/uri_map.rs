use crate::lsp::document_symbol::lsp::DocumentSymbol;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::RwLock;
use url::Url;

pub static URI_MAP: Lazy<RwLock<HashMap<Url, Vec<DocumentSymbol>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));
