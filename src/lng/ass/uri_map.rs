use dashmap::DashMap;
use once_cell::sync::Lazy;
use tree_sitter::{Parser, Tree};
use url::Url;

pub static PARSER_MAP: Lazy<DashMap<Url, Parser>> = Lazy::new(|| DashMap::new());
pub static TREE_MAP: Lazy<DashMap<Url, Tree>> = Lazy::new(|| DashMap::new());

