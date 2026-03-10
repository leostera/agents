use anyhow::{Result, anyhow};
use chrono::{DateTime, NaiveDateTime, Utc};

pub(crate) fn parse_ts(ts: &str) -> Result<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(ts) {
        return Ok(parsed.with_timezone(&Utc));
    }

    // Legacy SQLite rows may contain `YYYY-MM-DD HH:MM:SS[.fraction]` without timezone.
    if let Ok(naive) = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S%.f") {
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
    }

    Err(anyhow!("invalid RFC3339 timestamp"))
}
