use serde::{Deserialize, Serialize};

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#message
#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub jsonrpc: String,
}
