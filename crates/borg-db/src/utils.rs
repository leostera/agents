use anyhow::{Result, anyhow};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::Value;
use turso::Row;

use borg_core::{Task, TaskKind, TaskStatus, Uri};

pub(crate) fn row_to_task(row: &Row) -> Result<Task> {
    let status: String = row.get(2)?;
    let kind: String = row.get(3)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;

    let status =
        TaskStatus::from_str(&status).ok_or_else(|| anyhow!("invalid task status: {}", status))?;
    let kind = TaskKind::from_str(&kind).ok_or_else(|| anyhow!("invalid task kind: {}", kind))?;
    let task_id_raw: String = row.get(0)?;
    let parent_task_id_raw: Option<String> = row.get(1)?;
    let claimed_by_raw: Option<String> = row.get(7)?;

    Ok(Task {
        task_id: Uri::parse(&task_id_raw)?,
        parent_task_id: parent_task_id_raw.as_deref().map(Uri::parse).transpose()?,
        status,
        kind,
        payload: serde_json::from_str(&row.get::<String>(4)?).unwrap_or(Value::Null),
        created_at: parse_ts(&created_at)?,
        updated_at: parse_ts(&updated_at)?,
        claimed_by: claimed_by_raw.as_deref().map(Uri::parse).transpose()?,
        attempts: row.get(8)?,
        last_error: row.get(9)?,
    })
}

pub(crate) fn parse_ts(ts: &str) -> Result<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(ts) {
        return Ok(parsed.with_timezone(&Utc));
    }

    if let Ok(parsed) = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc));
    }

    if let Ok(epoch) = ts.parse::<i64>()
        && let Some(parsed) = DateTime::<Utc>::from_timestamp(epoch, 0)
    {
        return Ok(parsed);
    }

    Err(anyhow!("invalid timestamp `{}`", ts))
}
