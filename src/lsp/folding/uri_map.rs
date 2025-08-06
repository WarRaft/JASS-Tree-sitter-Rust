use crate::lsp::folding::lsp::FoldingRange;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::RwLock;
use url::Url;

pub static URI_MAP: Lazy<RwLock<HashMap<Url, Vec<FoldingRange>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));
