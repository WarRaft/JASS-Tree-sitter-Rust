use serde::{Deserialize, Serialize};

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#position
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub line: u32,
    pub character: u32,
}
