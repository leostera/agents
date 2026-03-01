use crate::actor::ActorHandle;
use crate::mailbox::ActorCommand;
use crate::message::{BorgMessage, SessionOutput};
use crate::runtime::BorgRuntime;
use anyhow::anyhow;
use borg_core::Uri;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Clone)]
pub struct BorgSupervisor {
    runtime: Arc<BorgRuntime>,
    sessions: Arc<RwLock<HashMap<Uri, ActorHandle>>>,
}

impl BorgSupervisor {
    pub fn new(runtime: Arc<BorgRuntime>) -> Self {
        Self {
            runtime,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        info!("BorgSupervisor starting");
        Ok(())
    }

    pub async fn call(&self, msg: BorgMessage) -> anyhow::Result<SessionOutput> {
        let actor = self.ensure_actor(&msg.session_id).await?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        actor
            .mailbox
            .send(ActorCommand::Call(msg, tx))
            .await
            .map_err(|_| anyhow!("actor mailbox closed"))?;
        rx.await.map_err(|_| anyhow!("response channel closed"))?
    }

    pub async fn cast(&self, msg: BorgMessage) {
        if let Ok(actor) = self.ensure_actor(&msg.session_id).await {
            let _ = actor.mailbox.send(ActorCommand::Cast(msg)).await;
        }
    }

    async fn ensure_actor(&self, session_id: &Uri) -> anyhow::Result<ActorHandle> {
        let mut sessions = self.sessions.write().await;

        if let Some(actor) = sessions.get(session_id) {
            return Ok(actor.clone());
        }

        let actor = ActorHandle::spawn(session_id.clone(), self.runtime.clone()).await?;
        sessions.insert(session_id.clone(), actor.clone());

        Ok(actor)
    }

    pub async fn shutdown(&self) {
        info!("BorgSupervisor shutting down");
        let mut sessions = self.sessions.write().await;
        for (_, actor) in sessions.drain() {
            let _ = actor.mailbox.send(ActorCommand::Terminate).await;
            actor.task.abort();
        }
    }
}
