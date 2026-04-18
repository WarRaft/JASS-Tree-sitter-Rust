use crate::http::ref_map::DeclKey;
use url::Url;

/// Whether an imported symbol is a function/native or a variable/type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ImportedKind {
    Func,
    Var,
}

/// A symbol from an imported file that should be visible in the current file.
#[derive(Debug, Clone)]
pub struct ImportedSymbol {
    pub origin_uri: Url,
    pub name: String,
    pub kind: ImportedKind,
    pub origin_decl_key: Option<DeclKey>,
    pub return_type: Option<String>,
    pub type_name: Option<String>,
}

