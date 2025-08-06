use dashmap::DashMap;
use lapce_xi_rope::Rope;
use once_cell::sync::Lazy;
use url::Url;

pub static ROPE_MAP: Lazy<DashMap<Url, Rope>> = Lazy::new(|| DashMap::new());
