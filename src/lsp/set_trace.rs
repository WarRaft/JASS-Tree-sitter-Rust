use serde::{Deserialize, Serialize};

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#setTrace
#[derive(Debug, Serialize, Deserialize)]
pub struct SetTraceParams {
    pub value: TraceValue,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TraceValue {
    Off,
    Messages,
    Verbose,
}
