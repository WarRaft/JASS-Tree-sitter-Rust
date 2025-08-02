use std::collections::HashMap;
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
use tree_sitter::{Parser, Tree};
use url::Url;

pub static PARSER_MAP: Lazy<Mutex<HashMap<Url, Parser>>> = Lazy::new(|| Mutex::new(HashMap::new()));
pub static TREE_MAP: Lazy<Mutex<HashMap<Url, Tree>>> = Lazy::new(|| Mutex::new(HashMap::new()));