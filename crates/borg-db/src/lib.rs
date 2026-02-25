mod core;
mod migrations;
mod providers;
mod sessions;
mod tasks;
mod utils;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use turso::Connection;

use borg_core::TaskKind;

#[derive(Clone)]
pub struct BorgDb {
    conn: Connection,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewTask {
    pub kind: TaskKind,
    pub payload: Value,
    pub parent_task_id: Option<String>,
}
