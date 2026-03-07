use anyhow::Result;
use borg_agent::{BorgToolCall, BorgToolResult, Toolchain};
use borg_apps::BorgApps;
use borg_codemode::{CodeModeContext, CodeModeRuntime};
use borg_core::Uri;
use borg_db::{AppConnectionRecord, BorgDb};
use borg_fs::BorgFs;
use borg_memory::MemoryStore;
use borg_shellmode::ShellModeRuntime;
use std::collections::HashMap;

use crate::actor_context_manager::ActorContextManager;
use crate::llm_resolver::BorgLLMResolver;
use crate::tool_runner::build_exec_toolchain_with_context;

pub struct BorgRuntime {
    pub db: BorgDb,
    pub memory: MemoryStore,
    pub runtime: CodeModeRuntime,
    pub shell_runtime: ShellModeRuntime,
    pub files: BorgFs,
    pub llm_resolver: BorgLLMResolver,
    pub actor_context_manager: ActorContextManager,
}

impl BorgRuntime {
    pub fn new(
        db: BorgDb,
        memory: MemoryStore,
        runtime: CodeModeRuntime,
        shell_runtime: ShellModeRuntime,
        files: BorgFs,
    ) -> Self {
        let actor_context_manager = ActorContextManager::new(db.clone());
        Self {
            db: db.clone(),
            memory,
            runtime,
            shell_runtime,
            files,
            llm_resolver: BorgLLMResolver::new(db),
            actor_context_manager,
        }
    }

    pub async fn llm(&self) -> Result<borg_llm::BorgLLM> {
        self.llm_resolver.llm().await
    }

    pub async fn build_toolchain(
        &self,
        user_id: &Uri,
        actor_id: &Uri,
    ) -> Result<Toolchain<BorgToolCall, BorgToolResult>> {
        let context = self.code_mode_context_for_turn(user_id, actor_id).await?;
        let runtime_toolchain = build_exec_toolchain_with_context(
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

    async fn code_mode_context_for_turn(
        &self,
        user_id: &Uri,
        actor_id: &Uri,
    ) -> Result<CodeModeContext> {
        Ok(CodeModeContext {
            current_port_id: None,
            current_message_id: None,
            current_actor_id: Some(actor_id.clone()),
            current_user_id: Some(user_id.clone()),
            env: self.app_env_for_actor(user_id).await?,
        })
    }

    async fn app_env_for_actor(&self, current_user_id: &Uri) -> Result<HashMap<String, String>> {
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
    current_user_id: &Uri,
) -> Option<&'a AppConnectionRecord> {
    let mut owned = None;
    let mut shared = None;
    for connection in connections
        .iter()
        .filter(|connection| connection.status.trim().eq_ignore_ascii_case("connected"))
    {
        match connection.owner_user_id.as_ref() {
            Some(owner) if owner == current_user_id => {
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
