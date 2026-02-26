use anyhow::Result;
use borg_agent::{Agent, Message, Session, SessionContextManager, SessionEventPayload, ToolSpec};
use borg_core::{Uri, uri};
use borg_db::BorgDb;
use borg_rt::default_tool_specs;
use std::sync::Arc;

use crate::types::UserMessage;

#[derive(Clone)]
pub struct SessionManager {
    db: BorgDb,
    model: String,
}

impl SessionManager {
    pub fn new(db: BorgDb, model: String) -> Self {
        Self { db, model }
    }

    pub async fn session_for_task(&self, msg: &UserMessage) -> Result<Session> {
        let session_id = msg
            .session_id
            .clone()
            .unwrap_or_else(|| uri!("borg", "session"));

        let agent_id = self.resolve_agent_id(msg, &session_id).await?;
        let mut agent = Agent::load(&agent_id, &self.db).await?;
        if let Some(spec) = self.db.get_agent_spec(&agent_id).await? {
            let tools: Vec<ToolSpec> = serde_json::from_value(spec.tools)?;
            agent = agent
                .with_model(spec.model)
                .with_system_prompt(spec.system_prompt)
                .with_tools(tools);
        } else {
            agent = agent
                .with_model(self.model.clone())
                .with_tools(default_tool_specs());
        }

        let mut session = Session::new(session_id.clone(), agent, self.db.clone()).await?;
        if let Some((port, ctx)) = self.db.get_any_port_session_context(&session_id).await? {
            if port == "telegram" {
                session.set_context_manager(Arc::new(
                    SessionContextManager::for_telegram_session_context(ctx),
                ));
            }
        }
        Ok(session)
    }

    async fn resolve_agent_id(&self, msg: &UserMessage, session_id: &Uri) -> Result<Uri> {
        if let Some(agent_id) = &msg.agent_id {
            return Ok(agent_id.clone());
        }

        let messages = self.db.list_session_messages(session_id, 0, 64).await?;
        for message in messages {
            let Ok(message) = serde_json::from_value::<Message>(message) else {
                continue;
            };
            if let Message::SessionEvent {
                payload: SessionEventPayload::Started { agent_id },
                ..
            } = message
            {
                return Ok(agent_id);
            }
        }

        Ok(uri!("borg", "agent", "default"))
    }
}
