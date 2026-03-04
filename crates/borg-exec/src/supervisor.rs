use crate::actor::ActorHandle;
use crate::mailbox::ActorCommand;
use crate::mailbox_envelope::ActorMailboxEnvelope;
use crate::message::{BorgMessage, SessionOutput};
use crate::runtime::BorgRuntime;
use anyhow::anyhow;
use borg_agent::{BorgToolCall, BorgToolResult};
use borg_core::Uri;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc::Sender;
use tracing::{info, warn};

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
        self.replay_queued_actor_messages().await?;
        Ok(())
    }

    pub async fn call(
        &self,
        msg: BorgMessage,
    ) -> anyhow::Result<SessionOutput<BorgToolCall, BorgToolResult>> {
        self.call_with_progress(msg, None).await
    }

    pub async fn call_with_progress(
        &self,
        msg: BorgMessage,
        progress_tx: Option<Sender<SessionOutput<BorgToolCall, BorgToolResult>>>,
    ) -> anyhow::Result<SessionOutput<BorgToolCall, BorgToolResult>> {
        let actor_message_id = self
            .runtime
            .db
            .enqueue_actor_message(
                &msg.actor_id,
                "CALL",
                Some(&msg.session_id),
                &serde_json::to_value(ActorMailboxEnvelope::from_borg_message(&msg))?,
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
                progress_tx,
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
                &serde_json::to_value(ActorMailboxEnvelope::from_borg_message(&msg))?,
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

        let exists = self.runtime.db.get_actor(actor_id).await?.is_some();
        if !exists {
            return Err(anyhow!("actor spec not found for actor_id {}", actor_id));
        }

        let actor = ActorHandle::spawn(actor_id.clone(), self.runtime.clone()).await?;
        actors.insert(actor_id.clone(), actor.clone());
        drop(actors);

        self.dispatch_queued_actor_messages(actor_id, &actor)
            .await?;

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

    async fn replay_queued_actor_messages(&self) -> anyhow::Result<()> {
        let queued = self.runtime.db.list_queued_actor_messages(1000).await?;
        if queued.is_empty() {
            return Ok(());
        }
        let actor_ids: HashSet<Uri> = queued.into_iter().map(|row| row.actor_id).collect();
        info!(
            target: "borg_exec",
            actors = actor_ids.len(),
            "replaying queued actor mailbox messages on startup by ensuring actors"
        );
        for actor_id in actor_ids {
            if self.runtime.db.get_actor(&actor_id).await?.is_none() {
                warn!(
                    target: "borg_exec",
                    actor_id = %actor_id,
                    "actor spec missing for queued mailbox messages; leaving queued"
                );
                continue;
            }

            if let Err(err) = self.ensure_actor(&actor_id).await {
                warn!(
                    target: "borg_exec",
                    actor_id = %actor_id,
                    error = %err,
                    "failed to ensure actor during queued replay"
                );
            }
        }
        Ok(())
    }

    async fn dispatch_queued_actor_messages(
        &self,
        actor_id: &Uri,
        actor: &ActorHandle,
    ) -> anyhow::Result<()> {
        loop {
            let Some(row) = self.runtime.db.claim_next_actor_message(actor_id).await? else {
                return Ok(());
            };

            let env = match serde_json::from_value::<ActorMailboxEnvelope>(row.payload.clone()) {
                Ok(env) => env,
                Err(err) => {
                    let _ = self
                        .runtime
                        .db
                        .fail_actor_message(&row.actor_message_id, &format!("decode error: {err}"))
                        .await;
                    continue;
                }
            };
            let msg = match env.to_borg_message() {
                Ok(msg) => msg,
                Err(err) => {
                    let _ = self
                        .runtime
                        .db
                        .fail_actor_message(&row.actor_message_id, &format!("decode error: {err}"))
                        .await;
                    continue;
                }
            };
            if let Err(err) = actor
                .mailbox
                .send(ActorCommand::Cast {
                    actor_message_id: row.actor_message_id.clone(),
                    msg,
                })
                .await
            {
                warn!(
                    target: "borg_exec",
                    actor_message_id = %row.actor_message_id,
                    actor_id = %row.actor_id,
                    error = %err,
                    "failed to send replayed mailbox message"
                );
                let _ = self
                    .runtime
                    .db
                    .fail_actor_message(&row.actor_message_id, "mailbox send failed during replay")
                    .await;
            }
        }
    }
}
