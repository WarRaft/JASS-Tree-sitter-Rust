use crate::lsp::diagnostic::lsp::DocumentDiagnosticReport;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::Mutex;
use url::Url;

pub static URI_MAP: Lazy<Mutex<HashMap<Url, DocumentDiagnosticReport>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
