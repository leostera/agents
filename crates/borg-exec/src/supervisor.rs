use crate::actor::ActorHandle;
use crate::mailbox::ActorCommand;
use crate::message::{BorgMessage, SessionOutput};
use crate::runtime::BorgRuntime;
use anyhow::anyhow;
use borg_core::Uri;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

const STALE_IN_PROGRESS_SECONDS: u64 = 300;

#[derive(Clone)]
pub struct BorgSupervisor {
    runtime: Arc<BorgRuntime>,
    actors: Arc<RwLock<HashMap<Uri, ActorHandle>>>,
}

impl BorgSupervisor {
    pub fn new(runtime: Arc<BorgRuntime>) -> Self {
        Self {
            runtime,
            actors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        info!("BorgSupervisor starting");
        let failed = self
            .runtime
            .db
            .fail_stale_in_progress_messages(STALE_IN_PROGRESS_SECONDS)
            .await?;
        if failed > 0 {
            info!(
                target: "borg_exec",
                failed,
                "failed stale in-progress actor mailbox rows on startup"
            );
        }
        Ok(())
    }

    pub async fn call(&self, msg: BorgMessage) -> anyhow::Result<SessionOutput> {
        let actor_message_id = self
            .runtime
            .db
            .enqueue_actor_message(
                &msg.actor_id,
                "CALL",
                Some(&msg.session_id),
                &mailbox_payload(&msg),
                None,
                None,
            )
            .await?;
        let actor = self.ensure_actor(&msg.actor_id).await?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        if let Err(err) = actor
            .mailbox
            .send(ActorCommand::Call {
                actor_message_id: actor_message_id.clone(),
                msg,
                response_tx: tx,
            })
            .await
            .map_err(|_| anyhow!("actor mailbox closed"))
        {
            let _ = self
                .runtime
                .db
                .fail_actor_message(&actor_message_id, &err.to_string())
                .await;
            return Err(err);
        }
        rx.await.map_err(|_| anyhow!("response channel closed"))?
    }

    pub async fn cast(&self, msg: BorgMessage) -> anyhow::Result<()> {
        let actor_message_id = self
            .runtime
            .db
            .enqueue_actor_message(
                &msg.actor_id,
                "CAST",
                Some(&msg.session_id),
                &mailbox_payload(&msg),
                None,
                None,
            )
            .await?;
        let actor = self.ensure_actor(&msg.actor_id).await?;
        actor
            .mailbox
            .send(ActorCommand::Cast {
                actor_message_id: actor_message_id.clone(),
                msg,
            })
            .await
            .map_err(|_| anyhow!("actor mailbox closed"))
            .inspect_err(|err| {
                let db = self.runtime.db.clone();
                let actor_message_id = actor_message_id.clone();
                let err_text = err.to_string();
                tokio::spawn(async move {
                    let _ = db.fail_actor_message(&actor_message_id, &err_text).await;
                });
            })?;
        Ok(())
    }

    async fn ensure_actor(&self, actor_id: &Uri) -> anyhow::Result<ActorHandle> {
        let mut actors = self.actors.write().await;

        if let Some(actor) = actors.get(actor_id) {
            return Ok(actor.clone());
        }

        let actor = ActorHandle::spawn(actor_id.clone(), self.runtime.clone()).await?;
        actors.insert(actor_id.clone(), actor.clone());

        Ok(actor)
    }

    pub async fn shutdown(&self) {
        info!("BorgSupervisor shutting down");
        let mut actors = self.actors.write().await;
        for (_, actor) in actors.drain() {
            let _ = actor.mailbox.send(ActorCommand::Terminate).await;
            actor.task.abort();
        }
    }
}

fn mailbox_payload(msg: &BorgMessage) -> Value {
    json!({
        "actor_id": msg.actor_id.to_string(),
        "user_id": msg.user_id.to_string(),
        "session_id": msg.session_id.to_string(),
        "input": match &msg.input {
            crate::message::BorgInput::Chat { text } => json!({ "kind": "chat", "text": text }),
            crate::message::BorgInput::Command(command) => json!({ "kind": "command", "name": format!("{command:?}") }),
        }
    })
}
