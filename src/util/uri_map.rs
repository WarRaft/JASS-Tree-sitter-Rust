use crate::lsp::semantic_hub::SemanticTokenHub;
use crate::util::line_list::LineList;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::Mutex;
use tree_sitter::{Parser, Tree};
use url::Url;

pub static PARSER_MAP: Lazy<Mutex<HashMap<Url, Parser>>> = Lazy::new(|| Mutex::new(HashMap::new()));
pub static TREE_MAP: Lazy<Mutex<HashMap<Url, Option<Tree>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub static SEMANTIC_MAP: Lazy<Mutex<HashMap<Url, SemanticTokenHub>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub static LNG_MAP: Lazy<Mutex<HashMap<Url, Option<String>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub static LINE_LIST_MAP: Lazy<Mutex<HashMap<Url, LineList>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// pub static DIAGNOSTICS_MAP: Lazy<Mutex<HashMap<Url, Vec<String>>>> =
//     Lazy::new(|| Mutex::new(HashMap::new()));
// pub static SYMBOLS_MAP: Lazy<Mutex<HashMap<Url, Vec<String>>>> =
//     Lazy::new(|| Mutex::new(HashMap::new()));
// pub static COMMENTS_MAP: Lazy<Mutex<HashMap<Url, Vec<String>>>> =
//     Lazy::new(|| Mutex::new(HashMap::new()));
