use crate::lsp::semantic::hub::Hub;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::RwLock;
use url::Url;

pub static URI_MAP: Lazy<RwLock<HashMap<Url, Hub>>> = Lazy::new(|| RwLock::new(HashMap::new()));
