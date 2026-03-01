use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};

pub(crate) fn parse_ts(ts: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(ts)
        .map_err(|_| anyhow!("invalid RFC3339 timestamp"))?
        .with_timezone(&Utc))
}
