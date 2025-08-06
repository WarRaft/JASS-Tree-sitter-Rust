use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::RwLock;
use url::Url;

pub static LNG_MAP: Lazy<RwLock<HashMap<Url, String>>> = Lazy::new(|| RwLock::new(HashMap::new()));
