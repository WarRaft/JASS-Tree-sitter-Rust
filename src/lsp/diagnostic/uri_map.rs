use crate::lsp::diagnostic::lsp::DocumentDiagnosticReport;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use url::Url;

pub static URI_MAP: Lazy<DashMap<Url, DocumentDiagnosticReport>> = Lazy::new(|| DashMap::new());
