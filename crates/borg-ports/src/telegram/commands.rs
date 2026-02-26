use anyhow::Result;
use borg_cmd::{CommandRegistry, CommandRequest};
use borg_core::Uri;

use super::{TELEGRAM_START_GREETING, TelegramCommandState, TelegramPort};

pub(super) fn build_telegram_command_registry(
    state: TelegramCommandState,
) -> Result<CommandRegistry<TelegramCommandState, String>> {
    CommandRegistry::build(state)
        .add_command("start", |req| async move { Ok(command_start(req)) })
        .add_command("agent", |req| async move { command_agent(req).await })
        .add_command("port", |req| async move { Ok(command_port(req)) })
        .add_command("compact", |req| async move { command_compact(req).await })
        .add_command("participants", |req| async move { command_participants(req).await })
        .add_command("context", |req| async move { command_context(req).await })
        .add_command("reset", |req| async move { command_reset(req).await })
        .build()
}

fn command_start(req: CommandRequest<TelegramCommandState>) -> String {
    let _ = req;
    TELEGRAM_START_GREETING.to_string()
}

fn command_port(req: CommandRequest<TelegramCommandState>) -> String {
    TelegramPort::port_info(&req.state.message)
}

async fn command_agent(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = req.state.session_id()?;
    let (agent_id, model) = req.state.exec.agent_info_for_session(&session_id).await?;
    Ok(format!(
        "Agent: {}\nModel: {}\nSession: {}",
        agent_id, model, session_id
    ))
}

async fn command_compact(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = req.state.session_id()?;
    let kept = req.state.exec.compact_session(&session_id).await?;
    Ok(format!(
        "Compacted session. Kept {} context message(s).",
        kept
    ))
}

async fn command_participants(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = req.state.session_id()?;
    let ctx = req
        .state
        .exec
        .get_port_session_context("telegram", &session_id)
        .await?;
    Ok(TelegramPort::format_participants_message(
        ctx.as_ref(),
        &req.state.message,
    ))
}

async fn command_context(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = req.state.session_id()?;
    let context = req.state.exec.context_window_for_session(&session_id).await?;
    let dump = serde_json::to_string_pretty(&context)?;
    Ok(dump)
}

async fn command_reset(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = req.state.session_id()?;
    let deleted_messages = req.state.exec.clear_session_history(&session_id).await?;
    let _ = req
        .state
        .exec
        .clear_port_session_context("telegram", &session_id)
        .await?;
    Ok(format!(
        "Reset complete. Cleared {} message(s) and Telegram session context.",
        deleted_messages
    ))
}

impl TelegramCommandState {
    fn session_id(&self) -> Result<Uri> {
        Uri::from_parts(
            "borg",
            "session",
            Some(&format!("telegram_{}", self.message.chat.id.0)),
        )
        .map_err(Into::into)
    }
}
