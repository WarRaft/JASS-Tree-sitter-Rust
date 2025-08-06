use lapce_xi_rope::Rope;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::RwLock;
use url::Url;

pub static ROPE_MAP: Lazy<RwLock<HashMap<Url, Rope>>> = Lazy::new(|| RwLock::new(HashMap::new()));