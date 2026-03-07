use anyhow::{Result, anyhow};
use borg_agent::{
    ActorThread, Agent, BorgToolCall, BorgToolResult, ContextChunk, ContextManager, Message,
    StaticContextProvider, ToolSpec,
};
use borg_apps::{BorgApps, default_tool_specs as default_apps_tool_specs};
use borg_codemode::default_tool_specs as default_codemode_tool_specs;
use borg_core::Uri;
use borg_db::BorgDb;
use borg_fs::default_borg_fs_tool_specs;
use borg_memory::default_memory_tool_specs;
use borg_schedule::default_schedule_tool_specs;
use borg_shellmode::default_tool_specs as default_shell_mode_tool_specs;
use borg_taskgraph::default_taskgraph_tool_specs;

use crate::tool_runner::default_exec_admin_tool_specs;

const TELEGRAM_CONTEXT_PREFIX: &str = "TELEGRAM_CONTEXT_JSON: ";
const DEFAULT_ACTOR_BEHAVIOR_PROMPT: &str = r#"Actor messaging protocol:
- Inbound actor messages may start with `ACTOR_MESSAGE_META {...}`.
- If that metadata includes `reply_target_actor_id`, you MUST reply using `Actors-sendMessage` with:
  - `target_actor_id = reply_target_actor_id`
  - `in_reply_to_submission_id = submission_id` when present.
- Do not answer actor-originated requests only as plain assistant text; send the actor reply through `Actors-sendMessage`.
"#;

#[derive(Clone)]
pub struct ActorContextManager {
    db: BorgDb,
}

impl ActorContextManager {
    pub fn new(db: BorgDb) -> Self {
        Self { db }
    }

    pub async fn actor_thread_for_task(
        &self,
        requested_actor_id: Option<&Uri>,
    ) -> Result<ActorThread<BorgToolCall, BorgToolResult>> {
        let actor_id = self.resolve_actor_id(requested_actor_id).await?;
        let agent = self.resolve_agent_for_turn(&actor_id).await?;

        let mut actor_thread = ActorThread::new(actor_id.clone(), agent, self.db.clone()).await?;
        if let Some((port, ctx)) = self.db.get_any_port_actor_context(&actor_id).await?
            && port == "telegram"
        {
            let context_message = Message::System {
                content: format!("{}{}", TELEGRAM_CONTEXT_PREFIX, ctx),
            };
            let context_manager = ContextManager::builder()
                .add_provider(StaticContextProvider::new(vec![ContextChunk::pinned(
                    vec![context_message],
                )]))
                .build();
            actor_thread.set_context_manager(context_manager);
        }
        Ok(actor_thread)
    }

    pub async fn resolve_agent_for_turn(
        &self,
        actor_id: &Uri,
    ) -> Result<Agent<BorgToolCall, BorgToolResult>> {
        let default_tools = self.default_tools_for_actor().await?;
        let agent = Agent::load(actor_id, &self.db).await?;
        Ok(agent
            .with_behavior_prompt(DEFAULT_ACTOR_BEHAVIOR_PROMPT)
            .with_tools(default_tools))
    }

    async fn resolve_actor_id(&self, requested_actor_id: Option<&Uri>) -> Result<Uri> {
        if let Some(actor_id) = requested_actor_id {
            return Ok(actor_id.clone());
        }

        Err(anyhow!(
            "missing actor id when creating actor thread; actor must be resolved by caller"
        ))
    }

    async fn default_tools_for_actor(&self) -> Result<Vec<ToolSpec>> {
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
