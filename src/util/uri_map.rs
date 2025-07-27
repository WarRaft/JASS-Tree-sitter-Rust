use crate::lsp::semantic_hub::SemanticTokenHub;
use crate::util::line_list::LineList;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::Mutex;
use tree_sitter::{Parser, Tree};
use url::Url;

pub static URI_MAP: Lazy<Mutex<UriMap>> = Lazy::new(|| Mutex::new(UriMap::new()));

pub struct UriMap {
    pub parser: HashMap<Url, Parser>,
    pub tree: HashMap<Url, Option<Tree>>,
    pub semantic: HashMap<Url, SemanticTokenHub>,
    pub lng: HashMap<Url, Option<String>>,
    pub line_list: HashMap<Url, LineList>,
    //pub diagnostics: HashMap<Url, Vec<String>>,
    //pub symbols: HashMap<Url, Vec<String>>,
    //pub comments: HashMap<Url, Vec<String>>,
}

impl UriMap {
    pub fn new() -> Self {
        Self {
            parser: HashMap::new(),
            tree: HashMap::new(),
            semantic: HashMap::new(),
            lng: HashMap::new(),
            line_list: HashMap::new(),
            //diagnostics: HashMap::new(),
            //symbols: HashMap::new(),
            //comments: HashMap::new(),
        }
    }

    pub fn entry(&mut self, url: &Url) -> UriMapEntry<'_> {
        let semantic = self
            .semantic
            .entry(url.clone())
            .or_insert_with(SemanticTokenHub::new);
        let tree = self.tree.entry(url.clone()).or_insert(None);
        let lng = self.lng.entry(url.clone()).or_insert(None);
        let line_list = self
            .line_list
            .entry(url.clone())
            .or_insert_with(LineList::new);
        let parser = self.parser.entry(url.clone()).or_insert_with(|| {
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_bni::LANGUAGE.into())
                .unwrap();
            parser
        });

        UriMapEntry {
            parser,
            tree,
            semantic,
            lng,
            line_list,
            //diagnostics,
            //symbols,
            //comments,
        }
    }
}

pub struct UriMapEntry<'a> {
    pub semantic: &'a mut SemanticTokenHub,
    pub tree: &'a mut Option<Tree>,
    pub parser: &'a mut Parser,
    pub lng: &'a mut Option<String>,
    pub line_list: &'a mut LineList,
    //pub diagnostics: &'a mut Vec<String>,
    //pub symbols: &'a mut Vec<String>,
    //pub comments: &'a mut Vec<String>,
}
