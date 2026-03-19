use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::context::ContextChunk;
use crate::agent::error::AgentResult;
/// Input records captured by a storage adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StorageInput {
    Message(ContextChunk),
    Steer(ContextChunk),
    Cancel,
}

/// Event records captured by a storage adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StorageEvent {
    ContextWindowMaterialized {
        chunks: Vec<ContextChunk>,
    },
    RequestPrepared {
        request: Value,
    },
    ModelOutputItem {
        item: Value,
    },
    ToolCallRequested {
        call_id: String,
        name: String,
        args: Value,
    },
    ToolExecutionCompleted {
        call_id: String,
        result: Value,
    },
    Completed {
        reply: Value,
    },
    Cancelled,
}

/// Structured record emitted by agent instrumentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StorageRecord {
    InputReceived {
        turn: Option<u64>,
        input: StorageInput,
    },
    TurnStarted {
        turn: u64,
    },
    TurnQueued {
        turn: u64,
    },
    EventEmitted {
        turn: u64,
        event: StorageEvent,
    },
}

/// Sink for structured agent instrumentation records.
#[async_trait]
pub trait StorageAdapter: Send + Sync {
    async fn record(&self, record: StorageRecord) -> AgentResult<()>;
}

/// Storage adapter that discards all records.
pub struct NoopStorageAdapter;

#[async_trait]
impl StorageAdapter for NoopStorageAdapter {
    async fn record(&self, _record: StorageRecord) -> AgentResult<()> {
        Ok(())
    }
}

/// Storage adapter that keeps all records in memory.
#[derive(Default)]
pub struct InMemoryStorageAdapter {
    records: Mutex<Vec<StorageRecord>>,
}

impl InMemoryStorageAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn records(&self) -> Vec<StorageRecord> {
        self.records.lock().expect("storage records").clone()
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

#[async_trait]
impl StorageAdapter for InMemoryStorageAdapter {
    async fn record(&self, record: StorageRecord) -> AgentResult<()> {
        self.records.lock().expect("storage records").push(record);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        InMemoryStorageAdapter, NoopStorageAdapter, StorageAdapter, StorageEvent, StorageInput,
        StorageRecord,
    };
    use crate::agent::context::{ContextChunk, ContextStrategy};

    #[tokio::test]
    async fn in_memory_adapter_records_items_in_order() {
        let storage = InMemoryStorageAdapter::new();
        storage
            .record(StorageRecord::InputReceived {
                turn: Some(1),
                input: StorageInput::Message(ContextChunk::user_text(
                    ContextStrategy::Compactable,
                    "hello",
                )),
            })
            .await
            .expect("record input");
        storage
            .record(StorageRecord::EventEmitted {
                turn: 1,
                event: StorageEvent::Cancelled,
            })
            .await
            .expect("record event");

        assert_eq!(
            storage.records(),
            vec![
                StorageRecord::InputReceived {
                    turn: Some(1),
                    input: StorageInput::Message(ContextChunk::user_text(
                        ContextStrategy::Compactable,
                        "hello",
                    )),
                },
                StorageRecord::EventEmitted {
                    turn: 1,
                    event: StorageEvent::Cancelled,
                }
            ]
        );
    }

    #[tokio::test]
    async fn noop_adapter_accepts_records() {
        let storage = NoopStorageAdapter;
        storage
            .record(StorageRecord::InputReceived {
                turn: None,
                input: StorageInput::Cancel,
            })
            .await
            .expect("record");
    }
}
