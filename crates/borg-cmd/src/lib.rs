use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Result, anyhow};

type CommandFuture<R> = Pin<Box<dyn Future<Output = Result<R>> + Send>>;
type CommandHandler<S, R> = Arc<dyn Fn(CommandRequest<S>) -> CommandFuture<R> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct CommandRequest<S> {
    pub state: S,
    pub raw_input: String,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Clone)]
pub struct CommandRegistry<S, R>
where
    S: Clone + Send + Sync + 'static,
    R: Send + 'static,
{
    state: S,
    handlers: HashMap<String, CommandHandler<S, R>>,
}

pub struct CommandRegistryBuilder<S, R>
where
    S: Clone + Send + Sync + 'static,
    R: Send + 'static,
{
    state: S,
    handlers: HashMap<String, CommandHandler<S, R>>,
}

impl<S, R> CommandRegistry<S, R>
where
    S: Clone + Send + Sync + 'static,
    R: Send + 'static,
{
    pub fn build(state: S) -> CommandRegistryBuilder<S, R> {
        CommandRegistryBuilder {
            state,
            handlers: HashMap::new(),
        }
    }

    pub fn is_command(&self, input: &str) -> bool {
        parse_command(input).is_some()
    }

    pub async fn run(&self, input: &str) -> Result<Option<R>> {
        let Some((command, args)) = parse_command(input) else {
            return Ok(None);
        };
        let Some(handler) = self.handlers.get(&command) else {
            return Err(anyhow!("unknown command: /{}", command));
        };
        let req = CommandRequest {
            state: self.state.clone(),
            raw_input: input.to_string(),
            command,
            args,
        };
        let response = handler(req).await?;
        Ok(Some(response))
    }

    pub fn commands(&self) -> Vec<String> {
        let mut out: Vec<String> = self.handlers.keys().cloned().collect();
        out.sort();
        out
    }

    pub fn help(&self) -> String {
        let mut lines = vec!["Available commands:".to_string()];
        for command in self.commands() {
            lines.push(format!("/{command}"));
        }
        lines.join("\n")
    }
}

impl<S, R> CommandRegistryBuilder<S, R>
where
    S: Clone + Send + Sync + 'static,
    R: Send + 'static,
{
    pub fn add_command<F, Fut>(mut self, name: &str, handler: F) -> Self
    where
        F: Fn(CommandRequest<S>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<R>> + Send + 'static,
    {
        let normalized = normalize_name(name);
        let wrapped: CommandHandler<S, R> = Arc::new(move |req| Box::pin(handler(req)));
        self.handlers.insert(normalized, wrapped);
        self
    }

    pub fn build(self) -> Result<CommandRegistry<S, R>> {
        if self.handlers.is_empty() {
            return Err(anyhow!("command registry must contain at least one command"));
        }
        Ok(CommandRegistry {
            state: self.state,
            handlers: self.handlers,
        })
    }
}

fn parse_command(input: &str) -> Option<(String, Vec<String>)> {
    let mut tokens = input.split_whitespace();
    let token = tokens.next()?;
    if !token.starts_with('/') {
        return None;
    }
    let token = token.trim_start_matches('/');
    let command = token
        .split('@')
        .next()
        .map(normalize_name)
        .filter(|name| !name.is_empty())?;
    let args = tokens.map(ToString::to_string).collect();
    Some((command, args))
}

fn normalize_name(name: &str) -> String {
    name.trim().trim_start_matches('/').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dispatches_known_command() {
        let commands = CommandRegistry::build(7usize)
            .add_command("port", |req| async move {
                Ok(format!("{}:{}", req.command, req.state))
            })
            .build()
            .expect("build");

        let out = commands.run("/port").await.expect("run");
        assert_eq!(out, Some("port:7".to_string()));
    }

    #[tokio::test]
    async fn unknown_slash_command_returns_error() {
        let commands = CommandRegistry::build(())
            .add_command("help", |_req| async move { Ok("ok".to_string()) })
            .build()
            .expect("build");

        let err = commands.run("/wat").await.expect_err("unknown command");
        assert!(err.to_string().contains("unknown command"));
    }

    #[test]
    fn detects_telegram_at_suffix() {
        let parsed = parse_command("/help@borg_bot a b").expect("parsed");
        assert_eq!(parsed.0, "help");
        assert_eq!(parsed.1, vec!["a".to_string(), "b".to_string()]);
    }
}
