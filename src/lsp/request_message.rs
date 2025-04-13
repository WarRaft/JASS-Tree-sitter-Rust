use crate::lsp::message::Message;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#requestMessage
#[derive(Serialize, Deserialize, Debug)]
pub struct RequestMessage {
    #[serde(flatten)]
    pub base: Message,

    pub id: Value,
    pub method: String,
    pub params: Option<Value>,
}
