use crate::actor::ActorHandle;
use crate::mailbox::ActorCommand;
use crate::mailbox_envelope::ActorMailboxEnvelope;
use crate::message::{ActorOutput, BorgMessage};
use crate::runtime::BorgRuntime;
use anyhow::anyhow;
use borg_agent::{BorgToolCall, BorgToolResult};
use borg_core::Uri;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use tokio::time::{Duration, interval};
use tracing::{error, info, warn};

const STALE_IN_PROGRESS_SECONDS: u64 = 300;
const ACTOR_SYNC_INTERVAL_MS: u64 = 250;
const ACTOR_SYNC_LIMIT: usize = 1000;

#[derive(Clone)]
pub struct BorgSupervisor {
    runtime: Arc<BorgRuntime>,
    actors: Arc<RwLock<HashMap<Uri, ActorHandle>>>,
    actor_sync_task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl BorgSupervisor {
    pub fn new(runtime: Arc<BorgRuntime>) -> Self {
        Self {
            runtime,
            actors: Arc::new(RwLock::new(HashMap::new())),
            actor_sync_task: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn start(self: Arc<Self>) -> anyhow::Result<tokio::task::JoinHandle<()>> {
        info!("BorgSupervisor starting");
        let supervisor = self.clone();
        let handle = tokio::spawn(async move {
            if let Err(err) = supervisor.run().await {
                error!(
                    target: "borg_exec",
                    error = %err,
                    "BorgSupervisor failed, shutting down"
                );
            }
        });
        Ok(handle)
    }

    async fn run(self: Arc<Self>) -> anyhow::Result<()> {
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
        self.sync_running_actors_once().await?;
        self.start_actor_sync_loop().await;
        info!("BorgSupervisor initialized, running background loops");
        Ok(())
    }

    pub async fn call(
        &self,
        msg: BorgMessage,
    ) -> anyhow::Result<ActorOutput<BorgToolCall, BorgToolResult>> {
        self.call_with_progress(msg, None).await
    }

    pub async fn call_with_progress(
        &self,
        msg: BorgMessage,
        progress_tx: Option<Sender<ActorOutput<BorgToolCall, BorgToolResult>>>,
    ) -> anyhow::Result<ActorOutput<BorgToolCall, BorgToolResult>> {
        let msg = msg;
        let actor_message_id = self
            .runtime
            .db
            .enqueue_actor_message_in_progress(
                &msg.actor_id,
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
                sender_actor_id: None,
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
        let msg = msg;
        let actor_message_id = self
            .runtime
            .db
            .enqueue_actor_message_in_progress(
                &msg.actor_id,
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
                sender_actor_id: None,
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
        if let Some(task) = self.actor_sync_task.write().await.take() {
            task.abort();
        }
        let mut actors = self.actors.write().await;
        for (_, actor) in actors.drain() {
            let _ = actor.mailbox.send(ActorCommand::Terminate).await;
            actor.task.abort();
        }
    }

    async fn start_actor_sync_loop(&self) {
        if self.actor_sync_task.read().await.is_some() {
            return;
        }
        let supervisor = self.clone();
        let task = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(ACTOR_SYNC_INTERVAL_MS));
            loop {
                ticker.tick().await;
                if let Err(err) = supervisor.sync_running_actors_once().await {
                    warn!(
                        target: "borg_exec",
                        error = %err,
                        "actor sync loop failed"
                    );
                }
            }
        });
        *self.actor_sync_task.write().await = Some(task);
    }

    async fn sync_running_actors_once(&self) -> anyhow::Result<()> {
        let rows = self.runtime.db.list_actors(ACTOR_SYNC_LIMIT).await?;
        for actor in rows
            .into_iter()
            .filter(|actor| actor.status.trim().eq_ignore_ascii_case("RUNNING"))
        {
            let handle = match self.ensure_actor(&actor.actor_id).await {
                Ok(handle) => handle,
                Err(err) => {
                    warn!(
                        target: "borg_exec",
                        actor_id = %actor.actor_id,
                        error = %err,
                        "failed to ensure running actor from actor-sync"
                    );
                    continue;
                }
            };

            if let Err(err) = self
                .dispatch_queued_actor_messages(&actor.actor_id, &handle)
                .await
            {
                warn!(
                    target: "borg_exec",
                    actor_id = %actor.actor_id,
                    error = %err,
                    "failed to dispatch queued actor messages from actor-sync"
                );
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub async fn running_actor_ids(&self) -> Vec<Uri> {
        let actors = self.actors.read().await;
        actors.keys().cloned().collect()
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
            self.runtime.db.ack_actor_message(&row.actor_message_id).await?;

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
                    sender_actor_id: row.sender_actor_id.clone(),
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
