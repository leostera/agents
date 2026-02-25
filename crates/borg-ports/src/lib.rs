use anyhow::Result;
use axum::http::{HeaderMap, HeaderValue};
use borg_exec::{ExecEngine, InboxMessage};
use serde::{Deserialize, Serialize};

pub const BORG_SESSION_ID_HEADER: &str = "x-borg-session-id";

#[derive(Clone)]
pub struct HttpPort {
    exec: ExecEngine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpPortResponse {
    pub task_id: String,
    pub session_id: String,
}

impl HttpPort {
    pub fn new(exec: ExecEngine) -> Self {
        Self { exec }
    }

    pub async fn inbox(
        &self,
        headers: &HeaderMap,
        payload: InboxMessage,
    ) -> Result<(HttpPortResponse, Option<HeaderValue>)> {
        let requested_session_id = headers
            .get(BORG_SESSION_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);

        let (task_id, session_id) = self
            .exec
            .enqueue_user_message(payload, requested_session_id)
            .await?;

        let header = HeaderValue::from_str(&session_id).ok();
        Ok((
            HttpPortResponse {
                task_id,
                session_id,
            },
            header,
        ))
    }
}
