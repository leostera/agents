use anyhow::Result;
use borg_cmd::{CommandRegistry, CommandRequest};
use borg_core::{TelegramUserId, Uri};

use super::{TELEGRAM_START_GREETING, TelegramCommandState, TelegramPort};

const MODEL_COMMAND_USAGE: &str = "Usage: /model [model_name]";

pub(super) fn build_telegram_command_registry(
    state: TelegramCommandState,
) -> Result<CommandRegistry<TelegramCommandState, String>> {
    CommandRegistry::build(state)
        .add_command("start", |req| async move { Ok(command_start(req)) })
        .add_command("agent", |req| async move { command_agent(req).await })
        .add_command("model", |req| async move { command_model(req).await })
        .add_command("port", |req| async move { Ok(command_port(req)) })
        .add_command("compact", |req| async move { command_compact(req).await })
        .add_command("participants", |req| async move {
            command_participants(req).await
        })
        .add_command("context", |req| async move { command_context(req).await })
        .add_command("reset", |req| async move { command_reset(req).await })
        .build()
}

fn command_start(req: CommandRequest<TelegramCommandState>) -> String {
    let _ = req;
    TELEGRAM_START_GREETING.to_string()
}

fn command_port(req: CommandRequest<TelegramCommandState>) -> String {
    TelegramPort::port_info(&req.state.port_name, &req.state.message)
}

async fn command_agent(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = req.state.session_id().await?;
    let (agent_id, model) = req.state.exec.agent_info_for_session(&session_id).await?;
    Ok(format_agent_summary(&agent_id, &model, &session_id))
}

async fn command_model(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = req.state.session_id().await?;
    match parse_model_command_action(&req.args) {
        ModelCommandAction::Show => {
            let (agent_id, model) = req.state.exec.agent_info_for_session(&session_id).await?;
            Ok(format_agent_summary(&agent_id, &model, &session_id))
        }
        ModelCommandAction::Set(model) => {
            let (agent_id, model) = req
                .state
                .exec
                .set_model_for_session(&session_id, &model)
                .await?;
            Ok(format!(
                "Updated model to {} for agent {}.\nSession: {}",
                model, agent_id, session_id
            ))
        }
        ModelCommandAction::Usage => Ok(MODEL_COMMAND_USAGE.to_string()),
    }
}

async fn command_compact(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = req.state.session_id().await?;
    let kept = req.state.exec.compact_session(&session_id).await?;
    Ok(format!(
        "Compacted session. Kept {} context message(s).",
        kept
    ))
}

async fn command_participants(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = req.state.session_id().await?;
    let ctx = req
        .state
        .exec
        .get_port_session_context(&req.state.port_name, &session_id)
        .await?;
    Ok(TelegramPort::format_participants_message(
        ctx.as_ref(),
        &req.state.message,
    ))
}

async fn command_context(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = req.state.session_id().await?;
    let context = req
        .state
        .exec
        .context_window_for_session(&session_id)
        .await?;
    let dump = serde_json::to_string_pretty(&context)?;
    Ok(dump)
}

async fn command_reset(req: CommandRequest<TelegramCommandState>) -> Result<String> {
    let session_id = req.state.session_id().await?;
    let deleted_messages = req.state.exec.clear_session_history(&session_id).await?;
    let _ = req
        .state
        .exec
        .clear_port_session_context(&req.state.port_name, &session_id)
        .await?;
    Ok(format!(
        "Reset complete. Cleared {} message(s) and Telegram session context.",
        deleted_messages
    ))
}

impl TelegramCommandState {
    async fn session_id(&self) -> Result<Uri> {
        let conversation_key = self.conversation_key()?;
        self.exec
            .resolve_port_session_id(&self.port_name, &conversation_key)
            .await
    }

    fn conversation_key(&self) -> Result<Uri> {
        let chat_id = self.message.chat.id.0;
        let user_id = self
            .message
            .from
            .as_ref()
            .map(|user| user.id.0)
            .unwrap_or(chat_id as u64);
        Ok(TelegramUserId::from_sender_id(user_id).into_uri())
    }
}

fn format_agent_summary(agent_id: &Uri, model: &str, session_id: &Uri) -> String {
    format!(
        "Agent: {}\nModel: {}\nSession: {}",
        agent_id, model, session_id
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelCommandAction {
    Show,
    Set(String),
    Usage,
}

fn parse_model_command_action(args: &[String]) -> ModelCommandAction {
    match args {
        [] => ModelCommandAction::Show,
        [model] if !model.trim().is_empty() => ModelCommandAction::Set(model.trim().to_string()),
        [..] => ModelCommandAction::Usage,
    }
}

#[cfg(test)]
mod tests {
    use super::{ModelCommandAction, parse_model_command_action};

    #[test]
    fn parse_model_command_action_handles_show() {
        assert_eq!(parse_model_command_action(&[]), ModelCommandAction::Show);
    }

    #[test]
    fn parse_model_command_action_handles_set() {
        assert_eq!(
            parse_model_command_action(&["openai/gpt-4.1-mini".to_string()]),
            ModelCommandAction::Set("openai/gpt-4.1-mini".to_string())
        );
    }

    #[test]
    fn parse_model_command_action_handles_usage_for_invalid_shape() {
        assert_eq!(
            parse_model_command_action(&["first".to_string(), "second".to_string()]),
            ModelCommandAction::Usage
        );
    }
}
