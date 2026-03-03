use anyhow::{Result, anyhow};
use borg_core::Uri;
use borg_db::BorgDb;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{
    Agent, ContextManager, ContextWindow, Message, SessionEndStatus, SessionEventPayload,
    SessionOutput, SessionResult,
};

const AGENT_STARTED_EVENT: &str = "agent_started";
const AGENT_FINISHED_EVENT: &str = "agent_finished";

#[derive(Clone)]
pub struct Session<TToolCall = Value, TToolResult = Value> {
    pub session_id: Uri,
    pub agent: Agent<TToolCall, TToolResult>,
    db: BorgDb,
    context_manager: ContextManager<TToolCall, TToolResult>,
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
        let mut session = Self {
            session_id,
            agent,
            db,
            context_manager: ContextManager::default(),
            last_processed_len: 0,
            steering_messages: Vec::new(),
            follow_up_messages: Vec::new(),
        };

        session.last_processed_len = session
            .db
            .count_session_messages(&session.session_id)
            .await?;
        Ok(session)
    }

    pub async fn add_message(&mut self, message: Message<TToolCall, TToolResult>) -> Result<()> {
        let payload = serde_json::to_value(message)?;
        self.db
            .append_session_message(&self.session_id, &payload)
            .await?;
        Ok(())
    }

    pub fn set_context_manager(
        &mut self,
        context_manager: ContextManager<TToolCall, TToolResult>,
    ) {
        self.context_manager = context_manager;
    }

    pub async fn read_messages(
        &self,
        from: usize,
        limit: usize,
    ) -> Result<Vec<Message<TToolCall, TToolResult>>> {
        let payloads = self
            .db
            .list_session_messages(&self.session_id, from, limit)
            .await?;
        payloads
            .into_iter()
            .map(serde_json::from_value::<Message<TToolCall, TToolResult>>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| anyhow!(err))
    }

    pub async fn has_unprocessed_messages(&self) -> Result<bool> {
        let count = self.db.count_session_messages(&self.session_id).await?;
        Ok(count > self.last_processed_len)
    }

    pub async fn has_unprocessed_user_messages(&self) -> Result<bool> {
        let messages = self
            .read_messages(self.last_processed_len, usize::MAX)
            .await?;
        Ok(messages
            .into_iter()
            .any(|m| matches!(m, Message::User { .. } | Message::UserAudio { .. })))
    }

    pub async fn mark_processed(&mut self) -> Result<()> {
        self.last_processed_len = self.db.count_session_messages(&self.session_id).await?;
        Ok(())
    }

    pub async fn agent_started(&mut self) -> Result<()> {
        self.add_message(Message::SessionEvent {
            name: AGENT_STARTED_EVENT.to_string(),
            payload: SessionEventPayload::Started {
                agent_id: self.agent.agent_id.clone(),
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

    pub async fn build_context(&self) -> Result<ContextWindow<TToolCall, TToolResult>> {
        let messages = self.read_messages(0, usize::MAX).await?;
        self.context_manager
            .build_context(&self.agent, &messages)
            .await
    }

    pub async fn user_key(&self) -> Result<Uri> {
        let record = self
            .db
            .get_session(&self.session_id)
            .await?
            .ok_or_else(|| anyhow!("session not found: {}", self.session_id))?;
        record
            .users
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("session has no users: {}", self.session_id))
    }

    pub async fn record_provider_usage(&self, provider: &str, tokens_used: u64) -> Result<()> {
        self.db.record_provider_usage(provider, tokens_used).await
    }
}

pub type JsonSession = Session<Value, Value>;
