use dashmap::DashMap;
use once_cell::sync::Lazy;
use url::Url;

pub static LNG_URI_MAP: Lazy<DashMap<Url, String>> = Lazy::new(|| DashMap::new());
