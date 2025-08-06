use crate::lsp::semantic::hub::Hub;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use url::Url;

pub static URI_MAP: Lazy<DashMap<Url, Hub>> = Lazy::new(|| DashMap::new());
