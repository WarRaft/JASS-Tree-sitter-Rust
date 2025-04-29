use crate::lsp::semantic_hub::SemanticTokenHub;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use tree_sitter::Tree;
use url::Url;

pub static URI_MAP: Lazy<Mutex<UriMap>> = Lazy::new(|| Mutex::new(UriMap::new()));

#[derive(Debug)]
pub struct UriMap {
    pub semantic: HashMap<Url, SemanticTokenHub>,
    pub tree: Option<Tree>,
}

impl UriMap {
    pub fn new() -> Self {
        Self {
            semantic: HashMap::new(),
            tree: None,
        }
    }

    pub fn semantic(&mut self, url: &Url) -> &mut SemanticTokenHub {
        self.semantic
            .entry(url.clone())
            .or_insert_with(SemanticTokenHub::new)
    }
}
