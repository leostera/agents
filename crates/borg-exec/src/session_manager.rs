use anyhow::Result;
use borg_agent::{Agent, Session};
use borg_db::BorgDb;
use uuid::Uuid;

use crate::types::InboxMessage;

#[derive(Clone)]
pub struct SessionManager {
    db: BorgDb,
    model: String,
}

impl SessionManager {
    pub fn new(db: BorgDb, model: String) -> Self {
        Self { db, model }
    }

    pub async fn session_for_task(&self, msg: &InboxMessage) -> Result<Session> {
        let session_id = msg
            .session_id
            .clone()
            .unwrap_or_else(|| format!("borg:session:{}", Uuid::now_v7()));
        let agent = Agent::new("borg-default")
            .with_model(self.model.clone());
        let agent = Agent {
            agent_id: "borg:agent:default".to_string(),
            ..agent
        }
            .with_system_prompt(
                "You are Borg's agent runtime. Use tools as needed, then respond clearly.",
            );
        Session::new(session_id, agent, self.db.clone()).await
    }
}
