use anyhow::Result;
use borg_agent::Toolchain;
use borg_apps::BorgApps;
use borg_codemode::{CodeModeContext, CodeModeRuntime};
use borg_core::{Uri, uri};
use borg_db::{AppConnectionRecord, BorgDb};
use borg_fs::BorgFs;
use borg_memory::MemoryStore;
use borg_shellmode::ShellModeRuntime;
use serde_json::Value;
use std::collections::HashMap;

use crate::llm_resolver::BorgLLMResolver;
use crate::port_context::{PortContext, TelegramSessionContext};
use crate::session_manager::SessionManager;
use crate::tool_runner::build_exec_toolchain_with_context;
use crate::types::UserMessage;

pub struct BorgRuntime {
    pub db: BorgDb,
    pub memory: MemoryStore,
    pub runtime: CodeModeRuntime,
    pub shell_runtime: ShellModeRuntime,
    pub files: BorgFs,
    pub llm_resolver: BorgLLMResolver,
    pub session_manager: SessionManager,
}

impl BorgRuntime {
    pub fn new(
        db: BorgDb,
        memory: MemoryStore,
        runtime: CodeModeRuntime,
        shell_runtime: ShellModeRuntime,
        files: BorgFs,
    ) -> Self {
        let agent_model = String::new();
        let session_manager = SessionManager::new(db.clone(), agent_model);
        Self {
            db: db.clone(),
            memory,
            runtime,
            shell_runtime,
            files,
            llm_resolver: BorgLLMResolver::new(db),
            session_manager,
        }
    }

    pub async fn llm(&self) -> Result<borg_llm::BorgLLM> {
        self.llm_resolver.llm().await
    }

    pub async fn build_toolchain(
        &self,
        msg: &UserMessage,
        session_id: &Uri,
        agent_id: &Uri,
    ) -> Result<Toolchain> {
        let context = self
            .code_mode_context_for_turn(msg, session_id, agent_id)
            .await?;
        let allow_task_creation = !is_task_worker_message(msg);
        let runtime_toolchain = build_exec_toolchain_with_context(
            self.runtime.clone(),
            self.shell_runtime.clone(),
            context,
            self.memory.clone(),
            self.db.clone(),
            self.files.clone(),
            session_id.clone(),
            agent_id.clone(),
            allow_task_creation,
        )?;
        let apps = BorgApps::new(self.db.clone()).await?;
        let apps_toolchain = apps.as_toolchain()?;
        Ok(runtime_toolchain.merge(apps_toolchain)?)
    }

    pub async fn merge_port_context(
        &self,
        port: &str,
        session_id: &Uri,
        metadata: &Value,
    ) -> Result<()> {
        if port != "telegram" {
            return Ok(());
        }
        let maybe_existing = self
            .db
            .get_port_session_context("telegram", session_id)
            .await?;
        let mut ctx = match maybe_existing {
            Some(value) => TelegramSessionContext::from_json(value)?,
            None => TelegramSessionContext::default(),
        };
        ctx.merge_message_metadata(metadata)?;
        self.db
            .upsert_port_session_context("telegram", session_id, &ctx.to_json()?)
            .await?;
        Ok(())
    }

    pub async fn ensure_session_record(&self, msg: &UserMessage, session_id: &Uri) -> Result<()> {
        let existing = self.db.get_session(session_id).await?;
        let port = msg
            .metadata
            .as_object()
            .and_then(|obj| obj.get("port"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|value| Uri::parse(value).ok())
            .or_else(|| {
                msg.metadata
                    .as_object()
                    .and_then(|obj| obj.get("port"))
                    .and_then(Value::as_str)
                    .and_then(|value| Uri::from_parts("borg", "port", Some(value)).ok())
            })
            .or_else(|| existing.as_ref().map(|session| session.port.clone()))
            .unwrap_or_else(|| uri!("borg", "port", "runtime"));

        let mut users = existing
            .as_ref()
            .map(|session| session.users.clone())
            .unwrap_or_default();
        if !users.iter().any(|user| user == &msg.user_id) {
            users.push(msg.user_id.clone());
        }
        if users.is_empty() {
            users.push(msg.user_id.clone());
        }

        self.db.upsert_session(session_id, &users, &port).await?;

        Ok(())
    }

    async fn code_mode_context_for_turn(
        &self,
        msg: &UserMessage,
        session_id: &Uri,
        agent_id: &Uri,
    ) -> Result<CodeModeContext> {
        let metadata = msg.metadata.as_object();
        let port_name = metadata
            .and_then(|obj| obj.get("port"))
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let current_port_id = Uri::from_parts("borg", "port", Some(port_name)).ok();

        let chat_id = metadata
            .and_then(|obj| obj.get("chat_id"))
            .and_then(Value::as_i64);
        let message_id = metadata
            .and_then(|obj| obj.get("message_id"))
            .and_then(Value::as_i64);
        let current_message_id = match (chat_id, message_id) {
            (Some(chat), Some(message)) => Uri::from_parts(
                "borg",
                "message",
                Some(&format!("telegram_{chat}_{message}")),
            )
            .ok(),
            (_, Some(message)) => {
                Uri::from_parts("borg", "message", Some(&message.to_string())).ok()
            }
            _ => None,
        };

        Ok(CodeModeContext {
            current_port_id,
            current_message_id,
            current_session_id: Some(session_id.clone()),
            current_agent_id: Some(agent_id.clone()),
            current_user_id: Some(msg.user_id.clone()),
            env: self.app_env_for_session(&msg.user_id).await?,
        })
    }

    async fn app_env_for_session(&self, current_user_id: &Uri) -> Result<HashMap<String, String>> {
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

fn is_task_worker_message(msg: &UserMessage) -> bool {
    msg.metadata
        .as_object()
        .and_then(|obj| obj.get("port"))
        .and_then(Value::as_str)
        .is_some_and(|port| port.eq_ignore_ascii_case("taskgraph"))
}
