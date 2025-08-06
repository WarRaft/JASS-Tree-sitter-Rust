use crate::lsp::diagnostic::lsp::DocumentDiagnosticReport;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::RwLock;
use url::Url;

pub static URI_MAP: Lazy<RwLock<HashMap<Url, DocumentDiagnosticReport>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));
