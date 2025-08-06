use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tree_sitter::{Parser, Tree};
use url::Url;

pub static PARSER_MAP: Lazy<RwLock<HashMap<Url, Parser>>> = Lazy::new(|| RwLock::new(HashMap::new()));
pub static TREE_MAP: Lazy<RwLock<HashMap<Url, Tree>>> = Lazy::new(|| RwLock::new(HashMap::new()));