use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct InitializeParams {
    pub process_id: Option<i64>,
    pub root_path: Option<String>,
    pub capabilities: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitializeResult {
    pub capabilities: Value,
}
