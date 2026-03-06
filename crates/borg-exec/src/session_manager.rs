use anyhow::{Result, anyhow};
use borg_agent::{
    Agent, BorgToolCall, BorgToolResult, ContextChunk, ContextManager, Message, Session,
    StaticContextProvider, ToolSpec,
};
use borg_apps::{BorgApps, default_tool_specs as default_apps_tool_specs};
use borg_codemode::default_tool_specs as default_codemode_tool_specs;
use borg_core::{Uri, uri};
use borg_db::BorgDb;
use borg_fs::default_borg_fs_tool_specs;
use borg_memory::default_memory_tool_specs;
use borg_schedule::default_schedule_tool_specs;
use borg_shellmode::default_tool_specs as default_shell_mode_tool_specs;
use borg_taskgraph::default_taskgraph_tool_specs;

use crate::tool_runner::default_exec_admin_tool_specs;

const TELEGRAM_SESSION_CONTEXT_PREFIX: &str = "TELEGRAM_SESSION_CONTEXT_JSON: ";

#[derive(Clone)]
pub struct SessionManager {
    db: BorgDb,
}

impl SessionManager {
    pub fn new(db: BorgDb) -> Self {
        Self { db }
    }

    pub async fn session_for_task(
        &self,
        session_id: Option<Uri>,
        requested_actor_id: Option<&Uri>,
    ) -> Result<Session<BorgToolCall, BorgToolResult>> {
        let session_id = session_id.unwrap_or_else(|| uri!("borg", "session"));
        let actor_id = self
            .resolve_actor_id(requested_actor_id, &session_id)
            .await?;
        let agent = self.resolve_agent_for_turn(&actor_id).await?;

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
        actor_id: &Uri,
    ) -> Result<Agent<BorgToolCall, BorgToolResult>> {
        let default_tools = self.default_tools_for_session().await?;
        let agent = Agent::load(actor_id, &self.db).await?;
        Ok(agent.with_behavior_prompt("").with_tools(default_tools))
    }

    async fn resolve_actor_id(
        &self,
        requested_actor_id: Option<&Uri>,
        _session_id: &Uri,
    ) -> Result<Uri> {
        if let Some(actor_id) = requested_actor_id {
            return Ok(actor_id.clone());
        }

        Err(anyhow!(
            "missing actor id when creating session; actor must be resolved by caller"
        ))
    }

    async fn default_tools_for_session(&self) -> Result<Vec<ToolSpec>> {
        let apps = BorgApps::new(self.db.clone()).await?;

        let mut tools = Vec::new();
        tools.extend(default_codemode_tool_specs());
        tools.extend(default_shell_mode_tool_specs());
        tools.extend(default_memory_tool_specs());
        tools.extend(default_borg_fs_tool_specs());
        tools.extend(default_taskgraph_tool_specs());
        tools.extend(default_schedule_tool_specs());
        tools.extend(default_exec_admin_tool_specs());
        tools.extend(default_apps_tool_specs());
        tools.extend(apps.capability_tool_specs());
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        tools.dedup_by(|a, b| a.name == b.name);
        Ok(tools)
    }
}
