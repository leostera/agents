use anyhow::Result;
use async_trait::async_trait;
use axum::http::HeaderMap;
use borg_exec::{ExecEngine, InboxMessage};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const BORG_SESSION_ID_HEADER: &str = "x-borg-session-id";

#[derive(Clone)]
pub enum PortConfig {
    Http { exec: ExecEngine },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMessage {
    pub user_key: String,
    pub text: String,
    pub metadata: Value,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub error: Option<String>,
}

impl PortMessage {
    pub fn from_http(headers: &HeaderMap, payload: InboxMessage) -> Self {
        let requested_session_id = headers
            .get(BORG_SESSION_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);

        Self {
            user_key: payload.user_key,
            text: payload.text,
            metadata: payload.metadata,
            session_id: requested_session_id,
            task_id: None,
            error: None,
        }
    }
}

#[async_trait]
pub trait Port: Send + Sync {
    fn init(config: PortConfig) -> Result<Self>
    where
        Self: Sized;

    async fn handle_messages(&self, messages: Vec<PortMessage>) -> Vec<PortMessage>;
}

#[derive(Clone)]
pub struct HttpPort {
    exec: ExecEngine,
}

#[async_trait]
impl Port for HttpPort {
    fn init(config: PortConfig) -> Result<Self> {
        match config {
            PortConfig::Http { exec } => Ok(Self { exec }),
        }
    }

    async fn handle_messages(&self, messages: Vec<PortMessage>) -> Vec<PortMessage> {
        let mut out = Vec::with_capacity(messages.len());
        for message in messages {
            let inbox = InboxMessage {
                user_key: message.user_key.clone(),
                text: message.text.clone(),
                session_id: message.session_id.clone(),
                metadata: message.metadata.clone(),
            };

            let outbound = match self
                .exec
                .enqueue_user_message(inbox, message.session_id.clone())
                .await
            {
                Ok((task_id, session_id)) => PortMessage {
                    task_id: Some(task_id.to_string()),
                    session_id: Some(session_id),
                    error: None,
                    ..message
                },
                Err(err) => PortMessage {
                    task_id: None,
                    session_id: message.session_id,
                    error: Some(err.to_string()),
                    ..message
                },
            };
            out.push(outbound);
        }
        out
    }
}

pub fn init_http_port(exec: ExecEngine) -> Result<HttpPort> {
    HttpPort::init(PortConfig::Http { exec })
}
