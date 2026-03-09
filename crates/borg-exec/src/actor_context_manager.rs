use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_agent::{
    ActorThread, Agent, BorgToolCall, BorgToolResult, ContextChunk, ContextManager,
    ContextProvider, ContextWindow, Message, StaticContextProvider, ToolOutputEnvelope, ToolSpec,
};
use borg_apps::{BorgApps, default_tool_specs as default_apps_tool_specs};
use borg_codemode::default_tool_specs as default_codemode_tool_specs;
use borg_core::{ActorId, EndpointUri, MessagePayload, WorkspaceId};
use borg_db::BorgDb;
use borg_fs::default_borg_fs_tool_specs;
use borg_memory::default_memory_tool_specs;
use borg_schedule::default_schedule_tool_specs;
use borg_shellmode::default_tool_specs as default_shell_mode_tool_specs;
use borg_taskgraph::default_taskgraph_tool_specs;

use crate::tool_runner::default_exec_admin_tool_specs;

const TELEGRAM_CONTEXT_PREFIX: &str = "TELEGRAM_CONTEXT_JSON: ";
pub const DEFAULT_ACTOR_SYSTEM_PROMPT: &str = r#"Actor messaging protocol:
- Port-originated user messages arrive as JSON:
  `{"kind":"port_message","actor_id":"...","user_id":"...","text":"...","port_context":{...}}`
- Use the `text` field as the user message content; `port_context` provides transport context.
- Inbound actor messages arrive as a JSON object:
  `{"type":"actor_message","sender_actor_id":"...","reply_target_actor_id":"...","submission_id":"...","text":"..."}`
- Use the `text` field as the message content from the sender actor.
- If the payload includes `reply_target_actor_id`, you MUST reply using the `Actors-sendMessage` tool with:
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
        requested_actor_id: Option<&ActorId>,
    ) -> Result<ActorThread<BorgToolCall, BorgToolResult>> {
        let actor_id = self.resolve_actor_id(requested_actor_id).await?;
        let agent = self.resolve_agent_for_turn(&actor_id).await?;
        let workspace_id = WorkspaceId::from_id("default");

        let mut actor_thread =
            ActorThread::new(actor_id.clone(), workspace_id, agent, self.db.clone()).await?;

        let context_manager = self.build_context_manager(&actor_id).await?;
        actor_thread.set_context_manager(context_manager);

        Ok(actor_thread)
    }

    pub async fn build_context_manager(
        &self,
        actor_id: &ActorId,
    ) -> Result<ContextManager<BorgToolCall, BorgToolResult>> {
        let mut builder = ContextManager::builder();

        // 1. Add DB provider
        builder = builder.add_provider(DbContextProvider::new(
            self.db.clone(),
            actor_id.clone().into(),
        ));

        // 2. Add Telegram context if available
        if let Some((port, ctx)) = self.db.get_any_port_actor_context(&actor_id).await?
            && port == "telegram"
        {
            let context_message = Message::System {
                content: format!("{}{}", TELEGRAM_CONTEXT_PREFIX, ctx),
            };
            builder = builder.add_provider(StaticContextProvider::new(vec![ContextChunk::pinned(
                vec![context_message],
            )]));
        }

        Ok(builder.build())
    }

    pub async fn resolve_agent_for_turn(
        &self,
        actor_id: &ActorId,
    ) -> Result<Agent<BorgToolCall, BorgToolResult>> {
        let default_tools = self.default_tools_for_actor().await?;
        let actor = self
            .db
            .get_actor(actor_id)
            .await?
            .ok_or_else(|| anyhow!("actor not found: {}", actor_id))?;

        let model = actor
            .model
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                anyhow!(
                    "model not configured for actor {} (set one via /model <name>)",
                    actor.actor_id
                )
            })?;

        // RFD0033:
        // System Prompt = Durable protocol/structured JSON configuration
        // Behavior Prompt = Dynamic actor persona (stored in DB as system_prompt)
        Ok(Agent::new(actor_id.clone())
            .with_model(model)
            .with_system_prompt(DEFAULT_ACTOR_SYSTEM_PROMPT)
            .with_behavior_prompt(actor.system_prompt)
            .with_tools(default_tools))
    }

    pub async fn build_context_window(
        &self,
        actor_id: &ActorId,
    ) -> Result<ContextWindow<BorgToolCall, BorgToolResult>> {
        let agent = self.resolve_agent_for_turn(actor_id).await?;
        let context_manager = self.build_context_manager(actor_id).await?;
        context_manager.build_context(&agent, &[]).await
    }

    async fn resolve_actor_id(&self, requested_actor_id: Option<&ActorId>) -> Result<ActorId> {
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

pub struct DbContextProvider {
    db: BorgDb,
    endpoint_id: EndpointUri,
}

impl DbContextProvider {
    pub fn new(db: BorgDb, endpoint_id: EndpointUri) -> Self {
        Self { db, endpoint_id }
    }
}

#[async_trait]
impl ContextProvider<BorgToolCall, BorgToolResult> for DbContextProvider {
    async fn get_context(&self) -> Result<Vec<ContextChunk<BorgToolCall, BorgToolResult>>> {
        let records = self.db.list_messages(&self.endpoint_id, 100).await?;
        let mut messages = Vec::new();
        for record in records {
            let msg = match record.payload {
                MessagePayload::UserText(p) => Message::User { content: p.text },
                MessagePayload::AssistantText(p) => Message::Assistant { content: p.text },
                MessagePayload::FinalAssistantMessage(p) => Message::Assistant { content: p.text },
                MessagePayload::ToolCall(p) => Message::ToolCall {
                    tool_call_id: p.tool_call_id.to_string(),
                    name: p.tool_name,
                    arguments: serde_json::from_str(&p.arguments_json).unwrap_or_default(),
                },
                MessagePayload::ToolResult(p) => Message::ToolResult {
                    tool_call_id: p.tool_call_id.to_string(),
                    name: p.tool_name,
                    content: if p.is_error {
                        ToolOutputEnvelope::Error(p.result_json)
                    } else {
                        ToolOutputEnvelope::Ok(BorgToolResult::from(
                            serde_json::from_str::<serde_json::Value>(&p.result_json)
                                .unwrap_or_default(),
                        ))
                    },
                },
                _ => continue,
            };
            messages.push(msg);
        }
        Ok(vec![ContextChunk::compactable(messages)])
    }
}
