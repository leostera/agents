use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use borg_core::Uri;
use borg_db::BorgDb;
use borg_exec::{
    BorgInput, BorgMessage, BorgRuntime, BorgSupervisor, RuntimeToolCall, RuntimeToolResult,
    SessionOutput,
};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time;
use tracing::{error, info, warn};

use crate::PortMessage;
use crate::discord::DiscordPort;
use crate::message::PortInput;
use crate::port::Provider::{Discord, Telegram, Unknown};
use crate::telegram::TelegramPort;
use crate::{Port, PortConfig};

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(300);
const PORT_SCAN_LIMIT: usize = 1_000;
const PORT_CHANNEL_CAPACITY: usize = 256;

pub struct BorgPortsSupervisor {
    rt: Arc<BorgRuntime>,
    sup: Arc<BorgSupervisor>,
    ports: HashMap<Uri, (PortConfig, JoinHandle<()>)>,
}

#[derive(Debug, Clone)]
struct RunningPortState {
    config: PortConfig,
    is_finished: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReconcileAction {
    Stop(Uri),
    Restart(Uri),
    Start(Uri),
}

impl BorgPortsSupervisor {
    pub fn new(rt: Arc<BorgRuntime>, sup: Arc<BorgSupervisor>) -> Self {
        let ports = HashMap::default();
        Self { rt, sup, ports }
    }

    pub async fn start(mut self) -> Result<()> {
        let mut ticker = time::interval(DEFAULT_POLL_INTERVAL);
        loop {
            ticker.tick().await;
            if let Err(err) = self.reconcile_now().await {
                error!(target: "borg_ports", error = %err, "ports supervisor reconcile failed");
            }
        }
    }

    pub async fn reconcile_now(&mut self) -> Result<()> {
        let mut desired_ports = self.desired_ports().await?;
        let running_states: HashMap<Uri, RunningPortState> = self
            .ports
            .iter()
            .map(|(port_id, (config, task))| {
                (
                    port_id.clone(),
                    RunningPortState {
                        config: config.clone(),
                        is_finished: task.is_finished(),
                    },
                )
            })
            .collect();

        let actions = compute_reconcile_plan(&running_states, &desired_ports);

        for action in actions {
            match action {
                ReconcileAction::Stop(port_id) => {
                    if let Some((_, task)) = self.ports.remove(&port_id) {
                        task.abort();
                        info!(target: "borg_ports", port_id = %port_id, "port disabled");
                    }
                }
                ReconcileAction::Restart(port_id) => {
                    if let Some((_, task)) = self.ports.remove(&port_id) {
                        task.abort();
                    }
                    if let Some(config) = desired_ports.remove(&port_id) {
                        self.spawn_port(port_id, config).await?;
                    }
                }
                ReconcileAction::Start(port_id) => {
                    if let Some(config) = desired_ports.remove(&port_id) {
                        self.spawn_port(port_id, config).await?;
                    }
                }
            }
        }

        Ok(())
    }

    async fn desired_ports(&self) -> Result<HashMap<Uri, PortConfig>> {
        let mut desired = HashMap::new();
        let ports = self.rt.db.list_ports(PORT_SCAN_LIMIT).await?;

        for port_record in ports {
            if !port_record.enabled {
                continue;
            }
            let config = PortConfig::from_record(port_record)?;
            desired.insert(config.port_id.clone(), config);
        }

        Ok(desired)
    }

    async fn spawn_port(&mut self, port_id: Uri, config: PortConfig) -> Result<()> {
        if matches!(config.provider, Unknown) {
            let provider = format!("{:?}", config.provider);
            let task = tokio::spawn(async move { std::future::pending::<()>().await });
            self.ports.insert(port_id.clone(), (config, task));
            warn!(
                target: "borg_ports",
                port_id = %port_id,
                provider = %provider,
                "port provider is not implemented yet; skipping startup"
            );
            return Ok(());
        }

        let (inbound_tx, inbound_rx): (Sender<PortMessage>, Receiver<PortMessage>) =
            mpsc::channel(PORT_CHANNEL_CAPACITY);
        let (outbound_tx, outbound_rx): (
            Sender<SessionOutput<RuntimeToolCall, RuntimeToolResult>>,
            Receiver<SessionOutput<RuntimeToolCall, RuntimeToolResult>>,
        ) = mpsc::channel(PORT_CHANNEL_CAPACITY);

        let sup = self.sup.clone();
        let db = self.rt.db.clone();
        let port_name = config.port_name.clone();
        let assigned_actor_id = config.assigned_actor_id.clone();
        let bridge_task = tokio::spawn(async move {
            bridge_loop(
                db,
                sup,
                port_name,
                assigned_actor_id,
                inbound_rx,
                outbound_tx,
            )
            .await;
        });

        let config_for_port_task = config.clone();
        let port_id_for_port_task = port_id.clone();
        let port_task = tokio::spawn(async move {
            match config_for_port_task.provider {
                Discord => match DiscordPort::new(config_for_port_task.clone()).await {
                    Ok(port) => port.run(inbound_tx, outbound_rx).await,
                    Err(err) => Err(err),
                },
                Telegram => match TelegramPort::new(config_for_port_task.clone()).await {
                    Ok(port) => port.run(inbound_tx, outbound_rx).await,
                    Err(err) => Err(err),
                },
                Unknown => {
                    warn!(target: "borg_ports", port_id = %port_id_for_port_task, "unknown port provider; skipping");
                    Ok(())
                }
            }
        });

        let task = tokio::spawn(async move {
            tokio::select! {
                result = bridge_task => {
                    match result {
                        Ok(()) => info!(target: "borg_ports", "bridge task exited"),
                        Err(err) => error!(target: "borg_ports", error = %err, "bridge task crashed"),
                    }
                }
                result = port_task => {
                    match result {
                        Ok(Ok(())) => info!(target: "borg_ports", "port task exited"),
                        Ok(Err(err)) => error!(target: "borg_ports", error = %err, "port task failed"),
                        Err(err) => error!(target: "borg_ports", error = %err, "port task crashed"),
                    }
                }
            }
        });

        self.ports.insert(port_id.clone(), (config, task));
        info!(target: "borg_ports", port_id = %port_id, "port enabled");
        Ok(())
    }
}

fn compute_reconcile_plan(
    current: &HashMap<Uri, RunningPortState>,
    desired: &HashMap<Uri, PortConfig>,
) -> Vec<ReconcileAction> {
    let current_port_set: HashSet<Uri> = current.keys().cloned().collect();
    let desired_port_set: HashSet<Uri> = desired.keys().cloned().collect();

    let mut actions = Vec::new();

    for port_id in current_port_set.difference(&desired_port_set) {
        actions.push(ReconcileAction::Stop(port_id.clone()));
    }

    for port_id in current_port_set.intersection(&desired_port_set) {
        let Some(running) = current.get(port_id) else {
            continue;
        };
        let Some(desired_config) = desired.get(port_id) else {
            continue;
        };

        if running.is_finished || running.config != *desired_config {
            actions.push(ReconcileAction::Restart(port_id.clone()));
        }
    }

    for port_id in desired_port_set.difference(&current_port_set) {
        actions.push(ReconcileAction::Start(port_id.clone()));
    }

    actions.sort_by(|a, b| action_key(a).cmp(&action_key(b)));
    actions
}

fn action_key(action: &ReconcileAction) -> (&'static str, String) {
    match action {
        ReconcileAction::Stop(port_id) => ("0", port_id.as_str().to_string()),
        ReconcileAction::Restart(port_id) => ("1", port_id.as_str().to_string()),
        ReconcileAction::Start(port_id) => ("2", port_id.as_str().to_string()),
    }
}

async fn bridge_loop(
    db: BorgDb,
    sup: Arc<BorgSupervisor>,
    port_name: String,
    assigned_actor_id: Option<Uri>,
    mut inbound_rx: Receiver<PortMessage>,
    outbound_tx: Sender<SessionOutput<RuntimeToolCall, RuntimeToolResult>>,
) {
    while let Some(message) = inbound_rx.recv().await {
        let session_id = match db
            .resolve_port_session(&port_name, &message.conversation_key, None)
            .await
        {
            Ok(value) => value,
            Err(err) => {
                error!(
                    target: "borg_ports",
                    error = %err,
                    port_name = %port_name,
                    "failed to resolve port session"
                );
                continue;
            }
        };
        let bound_actor_id = match db
            .resolve_port_actor(
                &port_name,
                &message.conversation_key,
                None,
                assigned_actor_id.as_ref(),
            )
            .await
        {
            Ok(value) => Some(value),
            Err(err) => {
                warn!(
                    target: "borg_ports",
                    error = %err,
                    port_name = %port_name,
                    conversation_key = %message.conversation_key,
                    "failed to resolve actor binding; falling back"
                );
                None
            }
        };
        if let Ok(ctx) = serde_json::to_value(&message.port_context) {
            if let Err(err) = db
                .upsert_port_session_context(&port_name, &session_id, &ctx)
                .await
            {
                warn!(
                    target: "borg_ports",
                    error = %err,
                    port_name = %port_name,
                    session_id = %session_id,
                    "failed to persist port session context snapshot"
                );
            }
        }

        let actor_id = select_actor_id(session_id.clone(), bound_actor_id);

        let output = match sup
            .call_with_progress(
                BorgMessage {
                    actor_id,
                    user_id: message.user_id,
                    session_id,
                    input: match message.input {
                        PortInput::Chat { text } => BorgInput::Chat { text },
                        PortInput::Audio {
                            file_id,
                            mime_type,
                            duration_ms,
                            language_hint,
                        } => BorgInput::Audio {
                            file_id,
                            mime_type,
                            duration_ms,
                            language_hint,
                        },
                        PortInput::Command(command) => BorgInput::Command(command),
                    },
                    port_context: message.port_context,
                },
                Some(outbound_tx.clone()),
            )
            .await
        {
            Ok(value) => value,
            Err(err) => {
                error!(
                    target: "borg_ports",
                    error = %err,
                    port_name = %port_name,
                    "failed to process port message"
                );
                continue;
            }
        };

        if outbound_tx.send(output).await.is_err() {
            break;
        }
    }
}

fn select_actor_id(session_id: Uri, bound_actor_id: Option<Uri>) -> Uri {
    if let Some(actor_id) = bound_actor_id {
        return actor_id;
    }
    session_id
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::{ReconcileAction, RunningPortState, compute_reconcile_plan, select_actor_id};
    use crate::port::{PortConfig, Privacy, Provider, Status};
    use borg_core::Uri;
    use borg_db::BorgDb;

    fn uri(value: &str) -> Uri {
        Uri::parse(value).expect("valid uri")
    }

    fn config(port_id: &str, name: &str, provider: Provider) -> PortConfig {
        PortConfig {
            port_id: uri(port_id),
            port_name: name.to_string(),
            provider,
            status: Status::Enabled,
            privacy: Privacy::Public,
            assigned_actor_id: None,
            settings_json: r#"{"bot_token":"test"}"#.to_string(),
        }
    }

    fn tmp_db_path(test_name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("unix epoch")
            .as_nanos();
        let pid = std::process::id();
        let mut path = std::env::temp_dir();
        path.push(format!(
            "borg-ports-supervisor-{test_name}-{pid}-{nanos}.db"
        ));
        path
    }

    #[test]
    fn select_actor_id_prefers_bound_actor_then_session() {
        let session = uri("borg:session:s1");
        let bound = uri("devmode:actor:bound");

        assert_eq!(select_actor_id(session.clone(), Some(bound.clone())), bound);
        assert_eq!(select_actor_id(session.clone(), None), session);
    }

    #[test]
    fn reconcile_plan_stops_removed_ports() {
        let mut current = HashMap::new();
        current.insert(
            uri("borg:port:telegram"),
            RunningPortState {
                config: config("borg:port:telegram", "telegram", Provider::Telegram),
                is_finished: false,
            },
        );
        let desired = HashMap::new();

        let actions = compute_reconcile_plan(&current, &desired);
        assert_eq!(
            actions,
            vec![ReconcileAction::Stop(uri("borg:port:telegram"))]
        );
    }

    #[test]
    fn reconcile_plan_starts_new_ports() {
        let current = HashMap::new();
        let mut desired = HashMap::new();
        desired.insert(
            uri("borg:port:telegram"),
            config("borg:port:telegram", "telegram", Provider::Telegram),
        );

        let actions = compute_reconcile_plan(&current, &desired);
        assert_eq!(
            actions,
            vec![ReconcileAction::Start(uri("borg:port:telegram"))]
        );
    }

    #[test]
    fn reconcile_plan_restarts_when_config_changes() {
        let mut current = HashMap::new();
        current.insert(
            uri("borg:port:telegram"),
            RunningPortState {
                config: config("borg:port:telegram", "telegram-old", Provider::Telegram),
                is_finished: false,
            },
        );
        let mut desired = HashMap::new();
        desired.insert(
            uri("borg:port:telegram"),
            config("borg:port:telegram", "telegram", Provider::Telegram),
        );

        let actions = compute_reconcile_plan(&current, &desired);
        assert_eq!(
            actions,
            vec![ReconcileAction::Restart(uri("borg:port:telegram"))]
        );
    }

    #[test]
    fn reconcile_plan_restarts_finished_ports() {
        let mut current = HashMap::new();
        current.insert(
            uri("borg:port:telegram"),
            RunningPortState {
                config: config("borg:port:telegram", "telegram", Provider::Telegram),
                is_finished: true,
            },
        );
        let mut desired = HashMap::new();
        desired.insert(
            uri("borg:port:telegram"),
            config("borg:port:telegram", "telegram", Provider::Telegram),
        );

        let actions = compute_reconcile_plan(&current, &desired);
        assert_eq!(
            actions,
            vec![ReconcileAction::Restart(uri("borg:port:telegram"))]
        );
    }

    #[test]
    fn reconcile_plan_starts_new_discord_port() {
        let current = HashMap::new();
        let mut desired = HashMap::new();
        desired.insert(
            uri("borg:port:discord"),
            config("borg:port:discord", "discord", Provider::Discord),
        );

        let actions = compute_reconcile_plan(&current, &desired);
        assert_eq!(
            actions,
            vec![ReconcileAction::Start(uri("borg:port:discord"))]
        );
    }

    #[tokio::test]
    async fn context_snapshot_roundtrips_through_port_bindings() {
        let path = tmp_db_path("context-roundtrip");
        let db = BorgDb::open_local(path.to_str().expect("db path"))
            .await
            .expect("open db");
        db.migrate().await.expect("migrate db");

        let session_id = uri("borg:session:session-1");
        let conversation_key = uri("telegram:chat:1");
        db.upsert_port_binding_full_record("telegram", &conversation_key, &session_id, None)
            .await
            .expect("seed binding");

        db.upsert_port_session_context(
            "telegram",
            &session_id,
            &serde_json::json!({"chat":{"id":1}}),
        )
        .await
        .expect("write context");

        let context = db
            .get_port_session_context("telegram", &session_id)
            .await
            .expect("read context");
        assert_eq!(context, Some(serde_json::json!({"chat":{"id":1}})));
    }
}
