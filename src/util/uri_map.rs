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
    pub tree: HashMap<Url, Option<Tree>>,

    pub diagnostics: HashMap<Url, Vec<String>>,
    pub symbols: HashMap<Url, Vec<String>>,
    pub comments: HashMap<Url, Vec<String>>,
}

impl UriMap {
    pub fn new() -> Self {
        Self {
            semantic: HashMap::new(),
            tree: HashMap::new(),
            diagnostics: HashMap::new(),
            symbols: HashMap::new(),
            comments: HashMap::new(),
        }
    }

    pub fn entry(&mut self, url: &Url) -> UriMapEntry<'_> {
        let semantic = self
            .semantic
            .entry(url.clone())
            .or_insert_with(SemanticTokenHub::new);
        let tree = self.tree.entry(url.clone()).or_insert(None);
        let diagnostics = self.diagnostics.entry(url.clone()).or_insert_with(Vec::new);
        let symbols = self.symbols.entry(url.clone()).or_insert_with(Vec::new);
        let comments = self.comments.entry(url.clone()).or_insert_with(Vec::new);

        UriMapEntry {
            semantic,
            tree,
            diagnostics,
            symbols,
            comments,
        }
    }
}

pub struct UriMapEntry<'a> {
    pub semantic: &'a mut SemanticTokenHub,
    pub tree: &'a mut Option<Tree>,
    pub diagnostics: &'a mut Vec<String>,
    pub symbols: &'a mut Vec<String>,
    pub comments: &'a mut Vec<String>,
}
