use anyhow::Result;
use borg_agent::{BorgToolCall, BorgToolResult, Toolchain};
use borg_apps::BorgApps;
use borg_codemode::{CodeModeContext, CodeModeRuntime};
use borg_core::{ActorId, EndpointUri, MessageId, MessagePayload, WorkspaceId};
use borg_db::{AppConnectionRecord, BorgDb};
use borg_fs::BorgFs;
use borg_memory::MemoryStore;
use borg_shellmode::ShellModeRuntime;
use std::collections::HashMap;
use std::sync::Arc;

use crate::actor_context_manager::ActorContextManager;
use crate::actor_manager::BorgActorManager;
use crate::llm_resolver::BorgLLMResolver;
use crate::tool_runner::build_exec_toolchain_with_context;

pub struct BorgRuntime {
    pub db: BorgDb,
    pub memory: MemoryStore,
    pub runtime: CodeModeRuntime,
    pub shell_runtime: ShellModeRuntime,
    pub files: BorgFs,
    pub llm_resolver: Arc<BorgLLMResolver>,
    pub actor_context_manager: ActorContextManager,
    supervisor: Arc<BorgActorManager>,
}

impl BorgRuntime {
    pub fn new(
        db: BorgDb,
        memory: MemoryStore,
        runtime: CodeModeRuntime,
        shell_runtime: ShellModeRuntime,
        files: BorgFs,
    ) -> Arc<Self> {
        let actor_context_manager = ActorContextManager::new(db.clone());
        let llm_resolver = Arc::new(BorgLLMResolver::new(db.clone()));

        let rt = Arc::new_cyclic(|_me| {
            let supervisor = BorgActorManager::new(db.clone());
            Self {
                db: db.clone(),
                memory,
                runtime,
                shell_runtime,
                files,
                llm_resolver: llm_resolver.clone(),
                actor_context_manager,
                supervisor: Arc::new(supervisor),
            }
        });

        // Start background tasks
        tokio::spawn(llm_resolver.clone().start_update_loop());
        let supervisor = rt.supervisor.clone();
        let rt_clone = rt.clone();
        tokio::spawn(async move {
            if let Err(err) = supervisor.start(rt_clone).await {
                tracing::error!(target: "borg_exec", error = %err, "supervisor failed to start");
            }
        });

        rt
    }

    pub fn supervisor(&self) -> &BorgActorManager {
        &self.supervisor
    }

    pub async fn llm(&self) -> Result<Arc<borg_llm::BorgLLM>> {
        self.llm_resolver.llm().await
    }

    /// Primary entry point for sending a message between Borg entities.
    pub async fn send_message(
        self: &Arc<Self>,
        sender_id: &EndpointUri,
        receiver_id: &EndpointUri,
        payload: MessagePayload,
    ) -> Result<MessageId> {
        let workspace_id = WorkspaceId::from_id("default");

        // 1. Deliver to DB
        let message_id = MessageId::new();
        self.db
            .insert_message(
                &message_id,
                &workspace_id,
                sender_id,
                receiver_id,
                &payload,
                None,
                None,
                None,
            )
            .await?;

        // 2. Notify receiver
        let record = self.db.get_message(&message_id).await?.ok_or_else(|| {
            anyhow::anyhow!("failed to fetch message after insertion: {}", message_id)
        })?;

        // If the receiver is an actor, we notify the supervisor.
        if receiver_id.as_str().starts_with("borg:actor:") {
            let actor_id = ActorId::parse(receiver_id.as_str())?;
            self.supervisor()
                .notify_running_actor(actor_id, record, self.clone())
                .await?;
        }

        Ok(message_id)
    }

    pub async fn build_toolchain(
        self: &Arc<Self>,
        user_id: &EndpointUri,
        actor_id: &ActorId,
    ) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
        let env = self.app_env_for_actor(user_id).await?;
        let context = CodeModeContext {
            current_port_id: None,
            current_message_id: None,
            current_actor_id: Some(actor_id.clone().into()),
            current_user_id: Some(user_id.clone().into()),
            env,
        };
        let runtime_toolchain = build_exec_toolchain_with_context(
            self.clone(),
            self.runtime.clone(),
            self.shell_runtime.clone(),
            context,
            self.memory.clone(),
            self.db.clone(),
            self.files.clone(),
            actor_id.clone(),
            user_id.clone(),
            true,
        )?;
        let apps = BorgApps::new(self.db.clone()).await?;
        let apps_toolchain = apps.as_toolchain()?;
        Ok(runtime_toolchain.merge(apps_toolchain)?)
    }

    async fn app_env_for_actor(
        &self,
        current_user_id: &EndpointUri,
    ) -> Result<HashMap<String, String>> {
        let mut env = HashMap::new();
        let apps = self.db.list_apps(500).await?;

        for app in apps
            .into_iter()
            .filter(|app| app.status.trim().eq_ignore_ascii_case("active"))
        {
            let connections = self.db.list_app_connections(&app.app_id, 500).await?;
            let Some(connection) = select_connection_for_user(&connections, current_user_id) else {
                continue;
            };
            let secrets = self
                .db
                .list_app_secrets(&app.app_id, Some(&connection.connection_id), 500)
                .await?;
            let available_secrets = app
                .available_secrets
                .into_iter()
                .filter(|name| !name.trim().is_empty())
                .collect::<Vec<_>>();
            if available_secrets.is_empty() {
                continue;
            }
            for available_secret in available_secrets {
                if let Some(secret_value) =
                    resolve_available_secret_value(&available_secret, &secrets)
                {
                    env.insert(available_secret, secret_value.to_string());
                }
            }
        }

        Ok(env)
    }
}

fn select_connection_for_user<'a>(
    connections: &'a [AppConnectionRecord],
    current_user_id: &EndpointUri,
) -> Option<&'a AppConnectionRecord> {
    let mut owned = None;
    let mut shared = None;
    let user_uri = current_user_id.as_uri();
    for connection in connections
        .iter()
        .filter(|connection| connection.status.trim().eq_ignore_ascii_case("connected"))
    {
        match connection.owner_user_id.as_ref() {
            Some(owner) if owner == user_uri => {
                if owned.is_none() {
                    owned = Some(connection);
                }
            }
            None => {
                if shared.is_none() {
                    shared = Some(connection);
                }
            }
            _ => {}
        }
    }
    owned.or(shared)
}

fn resolve_available_secret_value<'a>(
    available_secret: &str,
    secrets: &'a [borg_db::AppSecretRecord],
) -> Option<&'a str> {
    available_secret_candidate_keys(available_secret)
        .into_iter()
        .find_map(|candidate| {
            secrets
                .iter()
                .find(|secret| secret.key.eq_ignore_ascii_case(&candidate))
                .map(|secret| secret.value.as_str())
        })
}

fn available_secret_candidate_keys(available_secret: &str) -> Vec<String> {
    let trimmed = available_secret.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut keys = vec![trimmed.to_string(), trimmed.to_ascii_lowercase()];

    // Common OAuth secret rows are keyed as access_token/refresh_token/scope/expires_at.
    if let Some((_, suffix)) = trimmed.split_once('_') {
        keys.push(suffix.to_ascii_lowercase());
    }

    keys.sort();
    keys.dedup();
    keys
}
