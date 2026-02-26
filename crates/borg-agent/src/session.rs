use anyhow::{Result, anyhow};
use borg_core::Uri;
use borg_db::BorgDb;
use std::sync::Arc;

use crate::{
    Agent, CompactingContextManager, ContextManager, ContextWindow, Message, SessionEndStatus,
    SessionEventPayload, SessionOutput, SessionResult,
};

const AGENT_STARTED_EVENT: &str = "agent_started";
const AGENT_FINISHED_EVENT: &str = "agent_finished";

#[derive(Clone)]
pub struct Session {
    pub session_id: Uri,
    pub agent: Agent,
    db: BorgDb,
    context_manager: Arc<dyn ContextManager>,
    last_processed_len: usize,
    steering_messages: Vec<Message>,
    follow_up_messages: Vec<Message>,
}

impl Session {
    pub async fn new(session_id: Uri, agent: Agent, db: BorgDb) -> Result<Self> {
        let mut session = Self {
            session_id,
            agent,
            db,
            context_manager: Arc::new(CompactingContextManager::default()),
            last_processed_len: 0,
            steering_messages: Vec::new(),
            follow_up_messages: Vec::new(),
        };

        let existing_messages = session
            .db
            .count_session_messages(&session.session_id)
            .await?;
        if existing_messages == 0 && !session.agent.system_prompt.is_empty() {
            session
                .add_message(Message::System {
                    content: session.agent.system_prompt.clone(),
                })
                .await?;
        }
        session.last_processed_len = session
            .db
            .count_session_messages(&session.session_id)
            .await?;
        Ok(session)
    }

    pub async fn add_message(&mut self, message: Message) -> Result<()> {
        let payload = serde_json::to_value(message)?;
        self.db
            .append_session_message(&self.session_id, &payload)
            .await?;
        Ok(())
    }

    pub fn set_context_manager(&mut self, context_manager: Arc<dyn ContextManager>) {
        self.context_manager = context_manager;
    }

    pub async fn read_messages(&self, from: usize, limit: usize) -> Result<Vec<Message>> {
        let payloads = self
            .db
            .list_session_messages(&self.session_id, from, limit)
            .await?;
        payloads
            .into_iter()
            .map(serde_json::from_value::<Message>)
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
            .any(|m| matches!(m, Message::User { .. })))
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

    pub async fn agent_finished(&mut self, result: &SessionResult<SessionOutput>) -> Result<()> {
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

    pub async fn build_context(&self) -> Result<ContextWindow> {
        let messages = self.read_messages(0, usize::MAX).await?;
        self.context_manager
            .build_context(&self.agent, &messages)
            .await
    }
}
