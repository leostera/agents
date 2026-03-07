use anyhow::{Result, anyhow};
use borg_core::{Uri, uri};
use borg_db::BorgDb;
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::time::{Duration, Instant};

use crate::{
    ActorEventPayload, ActorRunOutput, ActorRunResult, ActorRunStatus, Agent, ContextManager,
    ContextWindow, Message,
};

const AGENT_STARTED_EVENT: &str = "agent_started";
const AGENT_FINISHED_EVENT: &str = "agent_finished";
const CONTEXT_CACHE_TTL: Duration = Duration::from_secs(60);
const HASH_MIX_CONSTANT: u64 = 0x9E37_79B1_85EB_CA87;

#[derive(Clone)]
struct CachedContext {
    content_hash: u64,
    built_at: Instant,
    context: ContextWindow,
}

#[derive(Clone)]
pub struct ActorThread {
    pub actor_id: Uri,
    pub agent: Agent,
    db: BorgDb,
    context_manager: ContextManager,
    messages: Vec<Message>,
    messages_hash: u64,
    cached_context: Option<CachedContext>,
    last_processed_len: usize,
    steering_messages: Vec<Message>,
    follow_up_messages: Vec<Message>,
}

impl ActorThread {
    pub async fn new(
        actor_id: Uri,
        agent: Agent,
        db: BorgDb,
    ) -> Result<Self> {
        let payloads = db.list_actor_messages(&actor_id, 0, usize::MAX).await?;
        let messages = payloads
            .into_iter()
            .map(serde_json::from_value::<Message>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| anyhow!(err))?;
        let messages_hash = Self::compute_messages_hash(&messages)?;
        let mut thread = Self {
            actor_id,
            agent,
            db,
            context_manager: ContextManager::default(),
            messages,
            messages_hash,
            cached_context: None,
            last_processed_len: 0,
            steering_messages: Vec::new(),
            follow_up_messages: Vec::new(),
        };

        thread.last_processed_len = thread.messages.len();
        Ok(thread)
    }

    pub async fn add_message(&mut self, message: Message) -> Result<()> {
        let payload = serde_json::to_value(message)?;
        let reasoning_effort = self.agent.reasoning_effort.map(|effort| effort.to_string());
        self.db
            .append_actor_history_message(&self.actor_id, &payload, reasoning_effort.as_deref())
            .await?;
        let appended = serde_json::from_value::<Message>(payload)
            .map_err(|err| anyhow!(err))?;
        self.messages_hash =
            Self::mix_hash(self.messages_hash, Self::hash_serializable(&appended)?);
        self.messages.push(appended);
        self.cached_context = None;
        Ok(())
    }

    pub fn set_context_manager(&mut self, context_manager: ContextManager) {
        self.context_manager = context_manager;
        self.cached_context = None;
    }

    pub async fn read_messages(
        &self,
        from: usize,
        limit: usize,
    ) -> Result<Vec<Message>> {
        if from >= self.messages.len() || limit == 0 {
            return Ok(Vec::new());
        }
        let end = from.saturating_add(limit).min(self.messages.len());
        Ok(self.messages[from..end].to_vec())
    }

    pub async fn has_unprocessed_messages(&self) -> Result<bool> {
        Ok(self.messages.len() > self.last_processed_len)
    }

    pub async fn has_unprocessed_user_messages(&self) -> Result<bool> {
        Ok(self
            .messages
            .iter()
            .skip(self.last_processed_len)
            .any(|m| matches!(m, Message::User { .. } | Message::UserAudio { .. })))
    }

    pub async fn mark_processed(&mut self) -> Result<()> {
        self.last_processed_len = self.messages.len();
        Ok(())
    }

    pub async fn agent_started(&mut self) -> Result<()> {
        self.add_message(Message::ActorEvent {
            name: AGENT_STARTED_EVENT.to_string(),
            payload: ActorEventPayload::Started {
                actor_id: self.agent.actor_id.clone(),
            },
        })
        .await
    }

    pub async fn agent_finished(
        &mut self,
        result: &ActorRunResult,
    ) -> Result<()> {
        let payload = match result {
            ActorRunResult::Completed(Ok(output)) => ActorEventPayload::Finished {
                status: ActorRunStatus::Completed,
                reply: Some(output.reply.clone()),
                error: None,
            },
            ActorRunResult::Completed(Err(err)) => ActorEventPayload::Finished {
                status: ActorRunStatus::CompletedError,
                reply: None,
                error: Some(err.clone()),
            },
            ActorRunResult::ActorError(err) => ActorEventPayload::Finished {
                status: ActorRunStatus::ActorError,
                reply: None,
                error: Some(err.clone()),
            },
            ActorRunResult::Idle => ActorEventPayload::Finished {
                status: ActorRunStatus::Idle,
                reply: None,
                error: None,
            },
        };
        self.add_message(Message::ActorEvent {
            name: AGENT_FINISHED_EVENT.to_string(),
            payload,
        })
        .await
    }

    pub fn enqueue_steering_message(&mut self, message: Message) {
        self.steering_messages.push(message);
    }

    pub fn enqueue_follow_up_message(&mut self, message: Message) {
        self.follow_up_messages.push(message);
    }

    pub fn pop_steering_messages(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.steering_messages)
    }

    pub fn pop_follow_up_messages(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.follow_up_messages)
    }

    pub async fn build_context(&mut self) -> Result<ContextWindow> {
        let content_hash = self.current_context_hash()?;
        if let Some(cache) = &self.cached_context
            && cache.content_hash == content_hash
            && cache.built_at.elapsed() < CONTEXT_CACHE_TTL
        {
            return Ok(cache.context.clone());
        }

        let context = self
            .context_manager
            .build_context(&self.agent, &self.messages)
            .await?;
        self.cached_context = Some(CachedContext {
            content_hash,
            built_at: Instant::now(),
            context: context.clone(),
        });
        Ok(context)
    }

    pub async fn user_key(&self) -> Result<Uri> {
        Ok(uri!("borg", "user", "system"))
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    pub async fn clear_history(&mut self) -> Result<usize> {
        let deleted = self.db.clear_actor_history(&self.actor_id).await?;
        self.messages.clear();
        self.messages_hash = 0;
        self.cached_context = None;
        self.last_processed_len = 0;
        Ok(usize::try_from(deleted).unwrap_or(usize::MAX))
    }

    pub async fn compact_history_keep_recent(&mut self, keep_messages: usize) -> Result<usize> {
        let normalized_keep = keep_messages.max(1);
        let total = self.messages.len();
        if total <= normalized_keep {
            return Ok(total);
        }

        // Preserve all currently unprocessed messages so retries never drop the active turn.
        let keep_start_by_count = total.saturating_sub(normalized_keep);
        let start = keep_start_by_count.min(self.last_processed_len);
        let kept_messages = self.messages[start..].to_vec();
        let reasoning_effort = self.agent.reasoning_effort.map(|effort| effort.to_string());

        self.db.clear_actor_history(&self.actor_id).await?;
        for message in &kept_messages {
            let payload = serde_json::to_value(message)?;
            self.db
                .append_actor_history_message(&self.actor_id, &payload, reasoning_effort.as_deref())
                .await?;
        }

        self.messages = kept_messages;
        self.messages_hash = Self::compute_messages_hash(&self.messages)?;
        self.cached_context = None;
        self.last_processed_len = self.last_processed_len.saturating_sub(start);
        Ok(self.messages.len())
    }

    pub async fn record_provider_usage(&self, provider: &str, tokens_used: u64) -> Result<()> {
        self.db.record_provider_usage(provider, tokens_used).await
    }

    pub async fn record_tool_call(
        &self,
        call_id: &str,
        tool_name: &str,
        arguments: &serde_json::Value,
        output: &crate::BorgToolResult,
    ) -> Result<()> {
        let arguments_json = serde_json::to_value(arguments)?;
        let output_json = serde_json::to_value(output)?;
        let success = output.output.clone();
        let is_ok = matches!(success, crate::ToolOutputEnvelope::Ok(_) | crate::ToolOutputEnvelope::ByDesign(_));
        let error_msg = match success {
            crate::ToolOutputEnvelope::Error(msg) => Some(msg),
            _ => None,
        };

        self.db
            .insert_tool_call(
                call_id,
                self.actor_id.as_str(),
                tool_name,
                &arguments_json,
                &output_json,
                is_ok,
                error_msg.as_deref(),
                None, // duration_ms
            )
            .await
    }

    fn current_context_hash(&self) -> Result<u64> {
        Ok(Self::mix_hash(
            self.messages_hash,
            self.hash_agent_signature()?,
        ))
    }

    fn hash_agent_signature(&self) -> Result<u64> {
        let mut hash = 0_u64;
        hash = Self::mix_hash(hash, Self::hash_str(&self.agent.actor_id.to_string()));
        hash = Self::mix_hash(hash, Self::hash_str(&self.agent.model));
        hash = Self::mix_hash(
            hash,
            Self::hash_str(
                self.agent
                    .reasoning_effort
                    .map(|effort| effort.as_str())
                    .unwrap_or(""),
            ),
        );
        hash = Self::mix_hash(hash, Self::hash_str(&self.agent.system_prompt));
        hash = Self::mix_hash(hash, Self::hash_str(&self.agent.behavior_prompt));
        for tool in &self.agent.tools {
            hash = Self::mix_hash(hash, Self::hash_str(&tool.name));
            hash = Self::mix_hash(hash, Self::hash_str(&tool.description));
            hash = Self::mix_hash(hash, Self::hash_str(&tool.parameters.to_string()));
        }
        Ok(hash)
    }

    fn compute_messages_hash(messages: &[Message]) -> Result<u64> {
        messages.iter().try_fold(0_u64, |hash, message| {
            Ok(Self::mix_hash(hash, Self::hash_serializable(message)?))
        })
    }

    fn hash_serializable<T: Serialize>(value: &T) -> Result<u64> {
        let encoded = serde_json::to_vec(value)?;
        let mut hasher = DefaultHasher::new();
        hasher.write(&encoded);
        Ok(hasher.finish())
    }

    fn hash_str(value: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(value.as_bytes());
        hasher.finish()
    }

    fn mix_hash(current: u64, next: u64) -> u64 {
        current.rotate_left(7) ^ next.wrapping_mul(HASH_MIX_CONSTANT)
    }
}
