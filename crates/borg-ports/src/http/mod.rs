use anyhow::{Result, anyhow};
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
            .and_then(|value| Uri::parse(value).ok())
            .or(payload.session_id);

        Self {
            port: "http".to_string(),
            user_key: payload.user_key,
            text: payload.text,
            metadata: payload.metadata,
            session_id: requested_session_id,
            agent_id: payload.agent_id,
            task_id: None,
            reply: None,
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
            _ => Err(anyhow!("invalid config for HttpPort")),
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

            let outbound = match self.exec.process_port_message(&message.port, inbox).await {
                Ok(output) => PortMessage {
                    task_id: None,
                    session_id: Some(output.session_id),
                    reply: output.reply,
                    error: None,
                    ..message
                },
                Err(err) => PortMessage {
                    task_id: None,
                    session_id: message.session_id,
                    reply: None,
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
