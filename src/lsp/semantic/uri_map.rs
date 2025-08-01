use crate::lsp::semantic::hub::Hub;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::Mutex;
use url::Url;

pub static URI_MAP: Lazy<Mutex<HashMap<Url, Hub>>> = Lazy::new(|| Mutex::new(HashMap::new()));
