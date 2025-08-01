use crate::lsp::document_symbol::lsp::DocumentSymbol;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::Mutex;
use url::Url;

pub static URI_MAP: Lazy<Mutex<HashMap<Url, Vec<DocumentSymbol>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
