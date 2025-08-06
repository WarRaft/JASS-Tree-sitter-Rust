use crate::lsp::folding::lsp::FoldingRange;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use url::Url;

pub static URI_MAP: Lazy<DashMap<Url, Vec<FoldingRange>>> = Lazy::new(|| DashMap::new());
