use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde_json::Value;
use turso::Row;

use borg_core::{Task, TaskKind, TaskStatus};

pub(crate) fn row_to_task(row: &Row) -> Result<Task> {
    let status: String = row.get(2)?;
    let kind: String = row.get(3)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;

    let status =
        TaskStatus::from_str(&status).ok_or_else(|| anyhow!("invalid task status: {}", status))?;
    let kind = TaskKind::from_str(&kind).ok_or_else(|| anyhow!("invalid task kind: {}", kind))?;

    Ok(Task {
        task_id: row.get(0)?,
        parent_task_id: row.get(1)?,
        status,
        kind,
        payload: serde_json::from_str(&row.get::<String>(4)?).unwrap_or(Value::Null),
        created_at: parse_ts(&created_at)?,
        updated_at: parse_ts(&updated_at)?,
        claimed_by: row.get(7)?,
        attempts: row.get(8)?,
        last_error: row.get(9)?,
    })
}

pub(crate) fn parse_ts(ts: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(ts)
        .map_err(|_| anyhow!("invalid RFC3339 timestamp"))?
        .with_timezone(&Utc))
}
