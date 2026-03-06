use anyhow::{Result, anyhow};
use borg_core::{Uri, uri};
use borg_db::BorgDb;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::time::{Duration, Instant};

use crate::{
    Agent, ContextManager, ContextWindow, Message, SessionEndStatus, SessionEventPayload,
    SessionOutput, SessionResult,
};

const AGENT_STARTED_EVENT: &str = "agent_started";
const AGENT_FINISHED_EVENT: &str = "agent_finished";
const CONTEXT_CACHE_TTL: Duration = Duration::from_secs(60);
const HASH_MIX_CONSTANT: u64 = 0x9E37_79B1_85EB_CA87;

#[derive(Clone)]
struct CachedContext<TToolCall, TToolResult> {
    content_hash: u64,
    built_at: Instant,
    context: ContextWindow<TToolCall, TToolResult>,
}

#[derive(Clone)]
pub struct Session<TToolCall, TToolResult> {
    pub session_id: Uri,
    pub agent: Agent<TToolCall, TToolResult>,
    db: BorgDb,
    context_manager: ContextManager<TToolCall, TToolResult>,
    messages: Vec<Message<TToolCall, TToolResult>>,
    messages_hash: u64,
    cached_context: Option<CachedContext<TToolCall, TToolResult>>,
    last_processed_len: usize,
    steering_messages: Vec<Message<TToolCall, TToolResult>>,
    follow_up_messages: Vec<Message<TToolCall, TToolResult>>,
}

impl<TToolCall, TToolResult> Session<TToolCall, TToolResult>
where
    TToolCall: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    TToolResult: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    pub async fn new(
        session_id: Uri,
        agent: Agent<TToolCall, TToolResult>,
        db: BorgDb,
    ) -> Result<Self> {
        let payloads = db.list_session_messages(&session_id, 0, usize::MAX).await?;
        let messages = payloads
            .into_iter()
            .map(serde_json::from_value::<Message<TToolCall, TToolResult>>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| anyhow!(err))?;
        let messages_hash = Self::compute_messages_hash(&messages)?;
        let mut session = Self {
            session_id,
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

        session.last_processed_len = session.messages.len();
        Ok(session)
    }

    pub async fn add_message(&mut self, message: Message<TToolCall, TToolResult>) -> Result<()> {
        let payload = serde_json::to_value(message)?;
        let reasoning_effort = self.agent.reasoning_effort.map(|effort| effort.to_string());
        self.db
            .append_session_message(&self.session_id, &payload, reasoning_effort.as_deref())
            .await?;
        let appended = serde_json::from_value::<Message<TToolCall, TToolResult>>(payload)
            .map_err(|err| anyhow!(err))?;
        self.messages_hash =
            Self::mix_hash(self.messages_hash, Self::hash_serializable(&appended)?);
        self.messages.push(appended);
        self.cached_context = None;
        Ok(())
    }

    pub fn set_context_manager(&mut self, context_manager: ContextManager<TToolCall, TToolResult>) {
        self.context_manager = context_manager;
        self.cached_context = None;
    }

    pub async fn read_messages(
        &self,
        from: usize,
        limit: usize,
    ) -> Result<Vec<Message<TToolCall, TToolResult>>> {
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
        self.add_message(Message::SessionEvent {
            name: AGENT_STARTED_EVENT.to_string(),
            payload: SessionEventPayload::Started {
                actor_id: self.agent.actor_id.clone(),
            },
        })
        .await
    }

    pub async fn agent_finished(
        &mut self,
        result: &SessionResult<SessionOutput<TToolCall, TToolResult>>,
    ) -> Result<()> {
        let payload = match result {
            SessionResult::Completed(Ok(output)) => SessionEventPayload::Finished {
                status: SessionEndStatus::Completed,
                reply: Some(output.reply.clone()),
                error: None,
            },
            SessionResult::Completed(Err(err)) => SessionEventPayload::Finished {
                status: SessionEndStatus::CompletedError,
                reply: None,
                error: Some(err.clone()),
            },
            SessionResult::SessionError(err) => SessionEventPayload::Finished {
                status: SessionEndStatus::SessionError,
                reply: None,
                error: Some(err.clone()),
            },
            SessionResult::Idle => SessionEventPayload::Finished {
                status: SessionEndStatus::Idle,
                reply: None,
                error: None,
            },
        };
        self.add_message(Message::SessionEvent {
            name: AGENT_FINISHED_EVENT.to_string(),
            payload,
        })
        .await
    }

    pub fn enqueue_steering_message(&mut self, message: Message<TToolCall, TToolResult>) {
        self.steering_messages.push(message);
    }

    pub fn enqueue_follow_up_message(&mut self, message: Message<TToolCall, TToolResult>) {
        self.follow_up_messages.push(message);
    }

    pub fn pop_steering_messages(&mut self) -> Vec<Message<TToolCall, TToolResult>> {
        std::mem::take(&mut self.steering_messages)
    }

    pub fn pop_follow_up_messages(&mut self) -> Vec<Message<TToolCall, TToolResult>> {
        std::mem::take(&mut self.follow_up_messages)
    }

    pub async fn build_context(&mut self) -> Result<ContextWindow<TToolCall, TToolResult>> {
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

    pub async fn record_provider_usage(&self, provider: &str, tokens_used: u64) -> Result<()> {
        self.db.record_provider_usage(provider, tokens_used).await
    }

    pub async fn record_tool_call(
        &self,
        call_id: &str,
        tool_name: &str,
        arguments: &TToolCall,
        output: &crate::ToolResultData<TToolResult>,
    ) -> Result<()> {
        let arguments_json = serde_json::to_value(arguments)?;
        let output_json = serde_json::to_value(output)?;
        let (success, error, duration_ms) = match output {
            crate::ToolResultData::Execution { duration, .. } => {
                let millis = duration.as_millis();
                let duration_ms = u64::try_from(millis).ok();
                (true, None, duration_ms)
            }
            crate::ToolResultData::Error { message } => (false, Some(message.clone()), None),
            _ => (true, None, None),
        };

        self.db
            .insert_tool_call(
                call_id,
                self.session_id.as_str(),
                tool_name,
                &arguments_json,
                &output_json,
                success,
                error.as_deref(),
                duration_ms,
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

    fn compute_messages_hash(messages: &[Message<TToolCall, TToolResult>]) -> Result<u64> {
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
