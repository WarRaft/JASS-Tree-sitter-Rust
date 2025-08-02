use crate::lsp::folding::lsp::FoldingRange;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::Mutex;
use url::Url;

pub static URI_MAP: Lazy<Mutex<HashMap<Url, Vec<FoldingRange>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
