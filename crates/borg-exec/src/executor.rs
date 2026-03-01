use anyhow::{Result, anyhow};
use borg_agent::{AgentTools, ContextWindow, Message, Session, SessionResult};
use borg_codemode::{CodeModeContext, CodeModeRuntime};
use borg_core::{TelegramUserId, Uri, uri};
use borg_db::BorgDb;
use borg_llm::BorgLLM;
use borg_llm::TranscriptionRequest;
use borg_llm::providers::openai::{OpenAiApiMode, OpenAiProvider};
use borg_llm::providers::openrouter::OpenRouterProvider;
use borg_memory::MemoryStore;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::port_context::{PortContext, TelegramSessionContext};
use crate::provider_config::ProviderConfigSnapshot;
use crate::session_manager::SessionManager;
use crate::tool_runner::build_exec_toolchain_with_context;
use crate::types::{SessionTurnOutput, ToolCallSummary, UserMessage};

const DEFAULT_AGENT_MODEL: &str = "gpt-4o-mini";
const CONTEXT_USAGE_CHAR_TO_TOKEN_RATIO: usize = 4;

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
}

fn parse_openai_mode(raw: Option<String>) -> Result<OpenAiApiMode> {
    let mode = raw
        .or_else(|| std::env::var("BORG_OPENAI_API_MODE").ok())
        .unwrap_or_else(|| "completions".to_string())
        .to_lowercase();
    match mode.as_str() {
        "chat" | "chat_completions" => Ok(OpenAiApiMode::ChatCompletions),
        "completions" => Ok(OpenAiApiMode::Completions),
        _ => Err(anyhow!(
            "unsupported OpenAI API mode `{}` (expected `chat_completions` or `completions`)",
            mode
        )),
    }
}

fn available_provider_names(settings: &ProviderConfigSnapshot) -> Vec<String> {
    let mut available = Vec::new();
    if normalize_optional(settings.openai_api_key.clone()).is_some() {
        available.push("openai".to_string());
    }
    if normalize_optional(settings.openrouter_api_key.clone()).is_some() {
        available.push("openrouter".to_string());
    }
    available
}

fn ordered_provider_names(preferred: &str, available: &[String]) -> Vec<String> {
    let mut ordered = Vec::with_capacity(available.len());
    if available.iter().any(|value| value == preferred) {
        ordered.push(preferred.to_string());
    }
    for name in available {
        if !ordered.iter().any(|value| value == name) {
            ordered.push(name.clone());
        }
    }
    ordered
}

fn build_openai_provider(settings: &ProviderConfigSnapshot) -> Result<Option<OpenAiProvider>> {
    let Some(api_key) = normalize_optional(settings.openai_api_key.clone()) else {
        return Ok(None);
    };
    let api_mode = parse_openai_mode(settings.openai_api_mode.clone())?;
    let provider = if let Some(base_url) = normalize_optional(settings.openai_base_url.clone()) {
        OpenAiProvider::new_with_base_url_and_mode(api_key, base_url, api_mode)
    } else {
        OpenAiProvider::new_with_mode(api_key, api_mode)
    };
    Ok(Some(provider))
}

fn build_openrouter_provider(
    settings: &ProviderConfigSnapshot,
) -> Result<Option<OpenRouterProvider>> {
    let Some(api_key) = normalize_optional(settings.openrouter_api_key.clone()) else {
        return Ok(None);
    };
    let provider = if let Some(base_url) = normalize_optional(settings.openrouter_base_url.clone())
    {
        OpenRouterProvider::new_with_base_url(api_key, base_url)
    } else if let Some(base_url) =
        normalize_optional(std::env::var("BORG_OPENROUTER_BASE_URL").ok())
    {
        OpenRouterProvider::new_with_base_url(api_key, base_url)
    } else {
        OpenRouterProvider::new(api_key)
    };
    Ok(Some(provider))
}

#[derive(Clone)]
pub struct BorgExecutor {
    db: BorgDb,
    memory: MemoryStore,
    runtime: CodeModeRuntime,
    worker_id: Uri,
    session_manager: SessionManager,
    openai_base_url: Option<String>,
    agent_model: String,
    provider_settings: Arc<RwLock<ProviderConfigSnapshot>>,
}

pub type ExecEngine = BorgExecutor;

impl BorgExecutor {
    pub fn new(db: BorgDb, memory: MemoryStore, runtime: CodeModeRuntime, worker_id: Uri) -> Self {
        let agent_model = DEFAULT_AGENT_MODEL.to_string();
        let session_manager = SessionManager::new(db.clone(), agent_model.clone());
        Self {
            db,
            memory,
            runtime,
            worker_id,
            session_manager,
            openai_base_url: None,
            agent_model,
            provider_settings: Arc::new(RwLock::new(ProviderConfigSnapshot::default())),
        }
    }

    pub fn with_openai_base_url(mut self, base_url: Option<String>) -> Self {
        self.openai_base_url = base_url;
        self
    }

    pub fn with_agent_model(mut self, model: impl Into<String>) -> Self {
        self.agent_model = model.into();
        self.session_manager = SessionManager::new(self.db.clone(), self.agent_model.clone());
        self
    }

    pub fn provider_settings_handle(&self) -> Arc<RwLock<ProviderConfigSnapshot>> {
        self.provider_settings.clone()
    }

    async fn configured_llm(&self) -> Result<BorgLLM> {
        let mut settings = self.provider_settings.read().await.clone();
        if self.openai_base_url.is_some() {
            settings.openai_base_url = self.openai_base_url.clone();
        }
        let available = available_provider_names(&settings);
        if available.is_empty() {
            return Err(anyhow!(
                "no configured provider is available for chat completion"
            ));
        }

        let preferred = settings
            .preferred_provider
            .clone()
            .unwrap_or_else(|| "openai".to_string())
            .to_lowercase();
        let ordered = ordered_provider_names(&preferred, &available);
        let mut builder = BorgLLM::build();
        for name in ordered {
            match name.as_str() {
                "openai" => {
                    if let Some(provider) = build_openai_provider(&settings)? {
                        builder = builder.add_provider(provider);
                    }
                }
                "openrouter" => {
                    if let Some(provider) = build_openrouter_provider(&settings)? {
                        builder = builder.add_provider(provider);
                    }
                }
                _ => {}
            }
        }
        Ok(builder.build()?)
    }

    pub async fn process_port_message(
        &self,
        port: &str,
        mut msg: UserMessage,
    ) -> Result<SessionTurnOutput> {
        let port_config = self.db.get_port(port).await?;
        if let Some(config) = &port_config {
            if !config.enabled {
                return Err(anyhow!("port is disabled"));
            }
            if !config.allows_guests
                && !is_allowed_external_user(config.provider.as_str(), &config.settings, &msg)
            {
                return Err(anyhow!("guest access is disabled for this port"));
            }
        }

        let (session_id, bound_agent_id) = self
            .db
            .resolve_port_session(
                port,
                &msg.user_key,
                msg.session_id.as_ref(),
                msg.agent_id.as_ref(),
            )
            .await?;
        msg.session_id = Some(session_id.clone());
        if msg.agent_id.is_none() {
            msg.agent_id = bound_agent_id.or_else(|| {
                port_config
                    .as_ref()
                    .and_then(|config| config.default_agent_id.clone())
            });
        }

        info!(
            target: "borg_exec",
            port,
            session_id = %session_id,
            user_key = %msg.user_key,
            "processing inbound port message on long-lived session"
        );

        self.merge_port_message_metadata(port, &session_id, &msg.metadata)
            .await?;
        let output = self.run_session_turn(&msg).await?;
        Ok(SessionTurnOutput {
            session_id,
            reply: output.as_ref().map(|value| value.reply.clone()),
            tool_calls: output
                .as_ref()
                .map(|value| {
                    value
                        .tool_calls
                        .iter()
                        .map(|call| ToolCallSummary {
                            tool_name: call.tool_name.clone(),
                            arguments: call.arguments.clone(),
                            output: serde_json::to_value(&call.output).unwrap_or_else(
                                |err| json!({ "Error": { "message": err.to_string() } }),
                            ),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    pub async fn estimate_session_context_usage_percent(
        &self,
        session_id: &Uri,
        max_context_tokens: usize,
    ) -> Result<usize> {
        let messages = self.db.list_session_messages(session_id, 0, 10_000).await?;
        let total_chars: usize = messages
            .iter()
            .map(|message| message.to_string().chars().count())
            .sum();
        let estimated_tokens = total_chars / CONTEXT_USAGE_CHAR_TO_TOKEN_RATIO;
        let max_tokens = max_context_tokens.max(1);
        let percent = ((estimated_tokens.saturating_mul(100)) / max_tokens).min(100);
        Ok(percent)
    }

    pub async fn list_session_messages(
        &self,
        session_id: &Uri,
        from: usize,
        limit: usize,
    ) -> Result<Vec<Value>> {
        self.db.list_session_messages(session_id, from, limit).await
    }

    pub async fn compact_session(&self, session_id: &Uri) -> Result<usize> {
        let context = self.context_window_for_session(session_id).await?;
        self.db.clear_session_history(session_id).await?;
        for message in &context.messages {
            let payload = serde_json::to_value(message)?;
            self.db.append_session_message(session_id, &payload).await?;
        }
        Ok(context.messages.len())
    }

    pub async fn transcribe_audio(
        &self,
        audio: Vec<u8>,
        mime_type: impl Into<String>,
    ) -> Result<String> {
        let llm = self.configured_llm().await?;
        let transcript = llm
            .audio_transcription(&TranscriptionRequest {
                audio,
                mime_type: mime_type.into(),
                model: None,
                language: None,
                prompt: None,
            })
            .await?;
        self.db.record_provider_usage("llm", 0).await?;
        Ok(transcript)
    }

    pub async fn context_window_for_session(&self, session_id: &Uri) -> Result<ContextWindow> {
        let session = self.session_for_session_id(session_id).await?;
        let context = session.build_context().await?;
        Ok(context)
    }

    pub async fn agent_info_for_session(&self, session_id: &Uri) -> Result<(Uri, String)> {
        let session = self.session_for_session_id(session_id).await?;
        Ok((session.agent.agent_id.clone(), session.agent.model.clone()))
    }

    pub async fn set_model_for_session(
        &self,
        session_id: &Uri,
        model: &str,
    ) -> Result<(Uri, String)> {
        let model = model.trim();
        if model.is_empty() {
            return Err(anyhow!("model must not be empty"));
        }

        let session = self.session_for_session_id(session_id).await?;
        let agent_id = session.agent.agent_id.clone();
        let (system_prompt, default_provider_id) =
            if let Some(spec) = self.db.get_agent_spec(&agent_id).await? {
                (spec.system_prompt, spec.default_provider_id)
            } else {
                (session.agent.system_prompt.clone(), None)
            };

        self.db
            .upsert_agent_spec(
                &agent_id,
                "Default Agent",
                default_provider_id.as_deref(),
                model,
                &system_prompt,
            )
            .await?;
        Ok((agent_id, model.to_string()))
    }

    pub async fn get_port_session_context(
        &self,
        port: &str,
        session_id: &Uri,
    ) -> Result<Option<Value>> {
        self.db.get_port_session_context(port, session_id).await
    }

    pub async fn upsert_port_session_context(
        &self,
        port: &str,
        session_id: &Uri,
        ctx: &Value,
    ) -> Result<()> {
        self.db
            .upsert_port_session_context(port, session_id, ctx)
            .await
    }

    pub async fn list_port_session_ids(&self, port: &str) -> Result<Vec<Uri>> {
        self.db.list_port_session_ids(port).await
    }

    pub async fn resolve_port_session_id(&self, port: &str, conversation_key: &Uri) -> Result<Uri> {
        let (session_id, _agent_id) = self
            .db
            .resolve_port_session(port, conversation_key, None, None)
            .await?;
        Ok(session_id)
    }

    pub async fn merge_port_message_metadata(
        &self,
        port: &str,
        session_id: &Uri,
        metadata: &Value,
    ) -> Result<()> {
        self.merge_port_context(port, session_id, metadata).await
    }

    pub async fn clear_session_history(&self, session_id: &Uri) -> Result<u64> {
        self.db.clear_session_history(session_id).await
    }

    pub async fn clear_port_session_context(&self, port: &str, session_id: &Uri) -> Result<u64> {
        self.db.clear_port_session_context(port, session_id).await
    }

    async fn session_for_session_id(&self, session_id: &Uri) -> Result<Session> {
        let synthetic_msg = UserMessage {
            user_key: Uri::from_parts("borg", "user", Some("system"))?,
            text: String::new(),
            session_id: Some(session_id.clone()),
            agent_id: None,
            metadata: json!({}),
        };
        self.session_manager.session_for_task(&synthetic_msg).await
    }

    pub async fn run(self) -> Result<()> {
        info!(
            target: "borg_exec",
            worker_id = %self.worker_id,
            "executor task loop disabled; waiting for shutdown"
        );
        std::future::pending::<()>().await;
        #[allow(unreachable_code)]
        Ok(())
    }

    async fn run_session_turn(
        &self,
        msg: &UserMessage,
    ) -> Result<Option<borg_agent::SessionOutput>> {
        let mut session = self.session_manager.session_for_task(msg).await?;
        let session_id = session.session_id.clone();
        self.ensure_session_record(msg, &session_id).await?;
        let code_mode_context = self.code_mode_context_for_turn(msg, &session_id);
        let toolchain = build_exec_toolchain_with_context(
            self.runtime.clone(),
            code_mode_context,
            self.memory.clone(),
        )?;
        let tools = AgentTools {
            tool_runner: &toolchain,
        };
        session
            .add_message(Message::User {
                content: msg.text.clone(),
            })
            .await?;
        let _context = session.build_context().await?;

        let llm = self.configured_llm().await?;

        let output = match session.agent.clone().run(&mut session, &llm, &tools).await {
            SessionResult::Completed(Ok(output)) => output,
            SessionResult::Completed(Err(err)) => {
                error!(
                    target: "borg_exec",
                    session_id = %session_id,
                    error = err.as_str(),
                    "agent session completed with error"
                );
                return Err(anyhow!("agent session completed with error: {}", err));
            }
            SessionResult::SessionError(err) => {
                error!(
                    target: "borg_exec",
                    session_id = %session_id,
                    error = err.as_str(),
                    "agent session errored"
                );
                return Err(anyhow!("agent session error: {}", err));
            }
            SessionResult::Idle => return Ok(None),
        };

        debug!(
            target: "borg_exec",
            session_id = %session_id,
            tool_calls = output.tool_calls.len(),
            "agent session completed"
        );
        for call in &output.tool_calls {
            let (success, error, duration_ms) = tool_call_outcome(&call.output);
            self.db
                .insert_tool_call(
                    &uri!("borg", "tool_call").to_string(),
                    &session_id.to_string(),
                    &call.tool_name,
                    &call.arguments,
                    &serde_json::to_value(&call.output)?,
                    success,
                    error.as_deref(),
                    duration_ms,
                )
                .await?;
        }
        info!(
            target: "borg_exec",
            session_id = %session_id,
            "agent session turn completed on long-lived session"
        );

        Ok(Some(output))
    }

    async fn ensure_session_record(&self, msg: &UserMessage, session_id: &Uri) -> Result<()> {
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
        if !users.iter().any(|user| user == &msg.user_key) {
            users.push(msg.user_key.clone());
        }
        if users.is_empty() {
            users.push(msg.user_key.clone());
        }

        self.db.upsert_session(session_id, &users, &port).await?;

        Ok(())
    }

    fn code_mode_context_for_turn(&self, msg: &UserMessage, session_id: &Uri) -> CodeModeContext {
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

        CodeModeContext {
            current_port_id,
            current_message_id,
            current_session_id: Some(session_id.clone()),
            current_user_id: Some(msg.user_key.clone()),
        }
    }

    async fn merge_port_context(
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
}

fn tool_call_outcome(output: &borg_agent::ToolResultData) -> (bool, Option<String>, Option<u64>) {
    match output {
        borg_agent::ToolResultData::Error { message } => (false, Some(message.clone()), None),
        borg_agent::ToolResultData::Execution { duration, .. } => {
            let ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
            (true, None, Some(ms))
        }
        _ => (true, None, None),
    }
}

fn is_allowed_external_user(provider: &str, settings: &Value, msg: &UserMessage) -> bool {
    let Some(allowed_ids) = settings
        .as_object()
        .and_then(|map| map.get("allowed_external_user_ids"))
        .and_then(Value::as_array)
    else {
        return false;
    };

    match provider.trim().to_ascii_lowercase().as_str() {
        "telegram" => {
            let candidates = telegram_external_user_candidates(msg);
            if candidates.is_empty() {
                return false;
            }

            allowed_ids
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|raw| raw.parse::<TelegramUserId>().ok())
                .any(|allowed| candidates.iter().any(|candidate| candidate == &allowed))
        }
        _ => false,
    }
}

fn telegram_external_user_candidates(msg: &UserMessage) -> Vec<TelegramUserId> {
    let mut out = Vec::new();
    let Some(metadata) = msg.metadata.as_object() else {
        return out;
    };

    if let Some(sender_id) = metadata.get("sender_id").and_then(Value::as_u64) {
        out.push(TelegramUserId::from_sender_id(sender_id));
    }
    if let Some(username) = metadata.get("sender_username").and_then(Value::as_str) {
        if let Some(parsed) = TelegramUserId::from_sender_username(username) {
            out.push(parsed);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use borg_core::uri;

    fn telegram_msg(sender_id: u64, sender_username: &str) -> UserMessage {
        UserMessage {
            user_key: uri!("telegram", "user", "example"),
            text: "hello".to_string(),
            metadata: json!({
                "sender_id": sender_id,
                "sender_username": sender_username,
            }),
            session_id: None,
            agent_id: None,
        }
    }

    #[test]
    fn telegram_allowlist_accepts_numeric_and_username_and_legacy() {
        let msg = telegram_msg(2654566, "leostera");
        let settings = json!({
            "allowed_external_user_ids": ["2654566", "@LEOSTERA", "telegram:user:2654566"]
        });
        assert!(is_allowed_external_user("telegram", &settings, &msg));
    }

    #[test]
    fn telegram_allowlist_rejects_non_matching_values() {
        let msg = telegram_msg(2654566, "leostera");
        let settings = json!({
            "allowed_external_user_ids": ["999999", "@someoneelse"]
        });
        assert!(!is_allowed_external_user("telegram", &settings, &msg));
    }
}
