use anyhow::Result;
use async_trait::async_trait;
use axum::http::HeaderMap;
use borg_core::Uri;
use borg_exec::{ExecEngine, UserMessage};

use crate::{Port, PortConfig, PortMessage};

pub const BORG_SESSION_ID_HEADER: &str = "x-borg-session-id";

impl PortMessage {
    pub fn from_http(headers: &HeaderMap, payload: UserMessage) -> Self {
        let requested_session_id = headers
            .get(BORG_SESSION_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| Uri::parse(value).ok());

        Self {
            user_key: payload.user_key,
            text: payload.text,
            metadata: payload.metadata,
            session_id: requested_session_id,
            agent_id: payload.agent_id,
            task_id: None,
            error: None,
        }
    }
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
            let inbox = UserMessage {
                user_key: message.user_key.clone(),
                text: message.text.clone(),
                session_id: message.session_id.clone(),
                agent_id: message.agent_id.clone(),
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
