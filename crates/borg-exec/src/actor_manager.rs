use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::{Duration, interval};
use tracing::{error, info, warn};

use crate::actor::ActorHandle;
use crate::mailbox::ActorCommand;
use crate::runtime::BorgRuntime;
use borg_core::{ActorId, EndpointUri, MessageId, MessagePayload, WorkspaceId};
use borg_db::{BorgDb, MessageRecord};

const ACTOR_SYNC_INTERVAL_MS: u64 = 500;
const ACTOR_SYNC_LIMIT: usize = 1000;

#[derive(Clone)]
pub struct BorgActorManager {
    db: BorgDb,
    actors: Arc<RwLock<HashMap<ActorId, ActorHandle>>>,
    actor_sync_task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl BorgActorManager {
    pub fn new(db: BorgDb) -> Self {
        Self {
            db,
            actors: Arc::new(RwLock::new(HashMap::new())),
            actor_sync_task: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn start(self: Arc<Self>, runtime: Arc<BorgRuntime>) -> Result<JoinHandle<()>> {
        info!("BorgActorManager starting");
        let supervisor = self.clone();
        let handle = tokio::spawn(async move {
            if let Err(err) = supervisor.run(runtime).await {
                error!(target: "borg_exec", error = %err, "BorgActorManager failed");
            }
        });
        Ok(handle)
    }

    async fn run(self: Arc<Self>, runtime: Arc<BorgRuntime>) -> Result<()> {
        self.sync_running_actors_once(runtime.clone()).await?;
        self.start_actor_sync_loop(runtime).await;
        info!("BorgActorManager initialized");
        Ok(())
    }

    /// Canonical way to send a message to an actor.
    /// This handles persistence (delivery) and notification.
    pub async fn send_to_actor(
        &self,
        workspace_id: &WorkspaceId,
        sender_id: &EndpointUri,
        receiver_id: &ActorId,
        payload: &MessagePayload,
        runtime: Arc<BorgRuntime>,
    ) -> Result<MessageId> {
        let message_id = MessageId::new();
        self.db
            .insert_message(
                &message_id,
                workspace_id,
                sender_id,
                &receiver_id.clone().into(),
                payload,
                None,
                None,
                None,
            )
            .await?;

        let record =
            self.db.get_message(&message_id).await?.ok_or_else(|| {
                anyhow!("failed to fetch message after insertion: {}", message_id)
            })?;

        self.notify_running_actor(receiver_id.clone(), record, runtime)
            .await?;

        Ok(message_id)
    }

    /// In-memory delivery of a message record to a running actor.
    pub async fn notify_running_actor(
        &self,
        actor_id: ActorId,
        record: MessageRecord,
        runtime: Arc<BorgRuntime>,
    ) -> Result<()> {
        let actor = self.ensure_actor(&actor_id, runtime).await?;
        let _ = actor.mailbox.send(ActorCommand::Message(record)).await;
        Ok(())
    }

    /// Requests the current effective context window from a running actor.
    /// Falls back to DB-reconstruction if the actor is not running or slow.
    pub async fn inspect_actor_context(
        &self,
        actor_id: &ActorId,
        runtime: Arc<BorgRuntime>,
    ) -> Result<borg_agent::ContextWindow<borg_agent::BorgToolCall, borg_agent::BorgToolResult>>
    {
        // 1. Try to get live context from running actor
        let maybe_actor = {
            let actors = self.actors.read().await;
            actors.get(actor_id).cloned()
        };

        if let Some(handle) = maybe_actor {
            let (tx, rx) = tokio::sync::oneshot::channel();
            if handle
                .mailbox
                .send(ActorCommand::InspectContext(tx))
                .await
                .is_ok()
            {
                // Wait for response with timeout
                match tokio::time::timeout(Duration::from_secs(2), rx).await {
                    Ok(Ok(Ok(context))) => return Ok(context),
                    Ok(Ok(Err(err))) => {
                        warn!(actor_id = %actor_id, error = %err, "actor failed to build context")
                    }
                    _ => {
                        warn!(actor_id = %actor_id, "actor context inspection timed out or failed")
                    }
                }
            }
        }

        // 2. Fallback to DB reconstruction
        runtime
            .actor_context_manager
            .build_context_window(actor_id)
            .await
    }

    async fn ensure_actor(
        &self,
        actor_id: &ActorId,
        runtime: Arc<BorgRuntime>,
    ) -> Result<ActorHandle> {
        {
            let actors = self.actors.read().await;
            if let Some(actor) = actors.get(actor_id) {
                return Ok(actor.clone());
            }
        }

        self.spawn(actor_id.clone(), runtime).await
    }

    pub async fn spawn(&self, actor_id: ActorId, runtime: Arc<BorgRuntime>) -> Result<ActorHandle> {
        let actor_record = self
            .db
            .get_actor(&actor_id)
            .await?
            .ok_or_else(|| anyhow!("actor not found: {}", actor_id))?;

        let handle =
            crate::actor::Actor::spawn(actor_id.clone(), actor_record.workspace_id, runtime)
                .await?;
        let mut actors = self.actors.write().await;
        actors.insert(actor_id, handle.clone());
        Ok(handle)
    }

    pub async fn shutdown(&self) {
        info!("BorgActorManager shutting down");
        if let Some(task) = self.actor_sync_task.write().await.take() {
            task.abort();
        }
        let mut actors = self.actors.write().await;
        for (_, actor) in actors.drain() {
            let _ = actor.mailbox.send(ActorCommand::Terminate).await;
            actor.abort().await;
        }
    }

    async fn start_actor_sync_loop(&self, runtime: Arc<BorgRuntime>) {
        if self.actor_sync_task.read().await.is_some() {
            return;
        }
        let supervisor = self.clone();
        let task = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(ACTOR_SYNC_INTERVAL_MS));
            loop {
                ticker.tick().await;
                if let Err(err) = supervisor.sync_running_actors_once(runtime.clone()).await {
                    warn!(target: "borg_exec", error = %err, "actor sync loop failed");
                }
            }
        });
        *self.actor_sync_task.write().await = Some(task);
    }

    async fn sync_running_actors_once(&self, runtime: Arc<BorgRuntime>) -> Result<()> {
        let rows = self.db.list_actors(ACTOR_SYNC_LIMIT).await?;
        for actor in rows
            .into_iter()
            .filter(|actor| actor.status.trim().eq_ignore_ascii_case("RUNNING"))
        {
            let handle = match self.ensure_actor(&actor.actor_id, runtime.clone()).await {
                Ok(handle) => handle,
                Err(err) => {
                    warn!(actor_id = %actor.actor_id, error = %err, "failed to ensure actor in sync");
                    continue;
                }
            };
            // Notify actor to check its mailbox
            let _ = handle.mailbox.send(ActorCommand::Notify).await;
        }
        Ok(())
    }
}
