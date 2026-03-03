use anyhow::Result;
use borg_agent::{
    Agent, ContextChunk, ContextManager, Message, Session, SessionEventPayload,
    StaticContextProvider, ToolSpec,
};
use borg_apps::BorgApps;
use borg_apps::default_tool_specs as default_apps_tool_specs;
use borg_clockwork::default_clockwork_tool_specs;
use borg_codemode::default_tool_specs;
use borg_core::{Uri, uri};
use borg_db::BorgDb;
use borg_memory::default_memory_tool_specs;
use serde_json::Value;
use borg_taskgraph::default_taskgraph_tool_specs;

use crate::tool_runner::default_exec_admin_tool_specs;

const TELEGRAM_SESSION_CONTEXT_PREFIX: &str = "TELEGRAM_SESSION_CONTEXT_JSON: ";

#[derive(Clone)]
pub struct SessionManager {
    db: BorgDb,
    model: String,
}

impl SessionManager {
    pub fn new(db: BorgDb, model: String) -> Self {
        Self { db, model }
    }

    pub async fn session_for_task(
        &self,
        session_id: Option<Uri>,
        requested_agent_id: Option<&Uri>,
    ) -> Result<Session<Value, Value>> {
        let session_id = session_id.unwrap_or_else(|| uri!("borg", "session"));
        let agent_id = self
            .resolve_agent_id(requested_agent_id, &session_id)
            .await?;
        let agent = self.resolve_agent_for_turn(&agent_id, None).await?;

        let mut session = Session::new(session_id.clone(), agent, self.db.clone()).await?;
        if let Some((port, ctx)) = self.db.get_any_port_session_context(&session_id).await?
            && port == "telegram"
        {
            let context_message = Message::System {
                content: format!("{}{}", TELEGRAM_SESSION_CONTEXT_PREFIX, ctx),
            };
            let context_manager = ContextManager::builder()
                .add_provider(StaticContextProvider::new(vec![ContextChunk::pinned(
                    vec![context_message],
                )]))
                .build();
            session.set_context_manager(context_manager);
        }
        Ok(session)
    }

    pub async fn resolve_agent_for_turn(
        &self,
        agent_id: &Uri,
        behavior_id: Option<&Uri>,
    ) -> Result<Agent<Value, Value>> {
        let default_tools = self.default_tools_for_session().await?;
        let mut agent = Agent::load(agent_id, &self.db).await?;
        if let Some(spec) = self.db.get_agent_spec(agent_id).await? {
            agent = agent
                .with_model(spec.model)
                .with_system_prompt(spec.system_prompt);
        } else {
            agent = agent.with_model(self.model.clone());
        }

        let behavior_prompt = if let Some(behavior_id) = behavior_id {
            self.db
                .get_behavior(behavior_id)
                .await?
                .map(|behavior| behavior.system_prompt)
                .unwrap_or_default()
        } else {
            String::new()
        };
        let behavior_prompt = if behavior_prompt.trim() == agent.system_prompt.trim() {
            String::new()
        } else {
            behavior_prompt
        };

        Ok(agent
            .with_behavior_prompt(behavior_prompt)
            .with_tools(default_tools))
    }

    async fn resolve_agent_id(
        &self,
        requested_agent_id: Option<&Uri>,
        session_id: &Uri,
    ) -> Result<Uri> {
        if let Some(agent_id) = requested_agent_id {
            return Ok(agent_id.clone());
        }

        let messages = self.db.list_session_messages(session_id, 0, 64).await?;
        for message in messages {
            let Ok(message) = serde_json::from_value::<Message<Value, Value>>(message) else {
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

        let specs = self.db.list_agent_specs(1).await?;
        if let Some(first) = specs.into_iter().next() {
            return Ok(first.agent_id);
        }

        Ok(uri!("borg", "agent", "default"))
    }

    async fn default_tools_for_session(&self) -> Result<Vec<ToolSpec>> {
        let apps = BorgApps::new(self.db.clone()).await?;
        Ok(ensure_default_tools(apps.capability_tool_specs()))
    }
}

fn ensure_default_tools(existing: Vec<ToolSpec>) -> Vec<ToolSpec> {
    let mut by_name: std::collections::BTreeMap<String, ToolSpec> = existing
        .into_iter()
        .map(|tool| (tool.name.clone(), tool))
        .collect();

    for tool in default_tool_specs()
        .into_iter()
        .chain(default_memory_tool_specs().into_iter())
        .chain(default_taskgraph_tool_specs().into_iter())
        .chain(default_clockwork_tool_specs().into_iter())
        .chain(default_exec_admin_tool_specs().into_iter())
        .chain(default_apps_tool_specs().into_iter())
    {
        by_name.insert(tool.name.clone(), tool);
    }

    by_name.into_values().collect()
}
