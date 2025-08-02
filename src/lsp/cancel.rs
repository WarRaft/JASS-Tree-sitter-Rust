use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tokio::sync::Mutex;

pub static CANCELLED_SET: Lazy<Mutex<HashSet<CancelId>>> = Lazy::new(|| Mutex::new(HashSet::new()));

/// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#cancelRequest
#[derive(Debug, Serialize, Deserialize)]
pub struct CancelParams {
    pub id: CancelId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(Eq, Hash, PartialEq)]
pub enum CancelId {
    Number(i64),
    String(String),
}

impl CancelId {
    pub async fn mark_cancelled(&self) {
        CANCELLED_SET.lock().await.insert(self.clone());
    }
}

#[async_trait::async_trait]
pub trait CancelCheck {
    async fn was_cancelled(&self) -> bool;
}

#[async_trait::async_trait]
impl CancelCheck for Option<CancelId> {
    async fn was_cancelled(&self) -> bool {
        match self {
            Some(id) => CANCELLED_SET.lock().await.remove(id),
            None => false,
        }
    }
}
