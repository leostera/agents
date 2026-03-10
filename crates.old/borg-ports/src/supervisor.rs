use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use borg_core::{ActorId, MessagePayload, PortId, WorkspaceId};
use borg_exec::BorgRuntime;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time;
use tracing::{error, info, warn};

use crate::PortMessage;
use crate::actor_id::deterministic_actor_id;
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
    ports: HashMap<PortId, (PortConfig, JoinHandle<()>)>,
}

#[derive(Debug, Clone)]
struct RunningPortState {
    config: PortConfig,
    is_finished: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReconcileAction {
    Stop(PortId),
    Restart(PortId),
    Start(PortId),
}

impl BorgPortsSupervisor {
    pub fn new(rt: Arc<BorgRuntime>) -> Self {
        let ports = HashMap::default();
        Self { rt, ports }
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
        let running_states: HashMap<PortId, RunningPortState> = self
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

    async fn desired_ports(&self) -> Result<HashMap<PortId, PortConfig>> {
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

    async fn spawn_port(&mut self, port_id: PortId, config: PortConfig) -> Result<()> {
        if matches!(config.provider, Unknown) {
            let task = tokio::spawn(async move { std::future::pending::<()>().await });
            self.ports.insert(port_id.clone(), (config, task));
            warn!(target: "borg_ports", port_id = %port_id, "unknown port provider; skipping");
            return Ok(());
        }

        let (inbound_tx, inbound_rx): (Sender<PortMessage>, Receiver<PortMessage>) =
            mpsc::channel(PORT_CHANNEL_CAPACITY);

        let (_outbound_tx, outbound_rx) = mpsc::channel(PORT_CHANNEL_CAPACITY);

        let bridge = BridgeLoop::new(
            self.rt.clone(),
            port_id.clone(),
            inbound_rx,
            config.assigned_actor_id.clone(),
        );
        let bridge_task = tokio::spawn(async move {
            bridge.run().await;
        });

        let config_for_port_task = config.clone();
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
                _ => Ok(()),
            }
        });

        let task = tokio::spawn(async move {
            tokio::select! {
                _ = bridge_task => info!(target: "borg_ports", "bridge task exited"),
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

pub struct BridgeLoop {
    rt: Arc<BorgRuntime>,
    port_id: PortId,
    inbound_rx: Receiver<PortMessage>,
    default_actor_id: Option<ActorId>,
}

impl BridgeLoop {
    pub fn new(
        rt: Arc<BorgRuntime>,
        port_id: PortId,
        inbound_rx: Receiver<PortMessage>,
        default_actor_id: Option<ActorId>,
    ) -> Self {
        Self {
            rt,
            port_id,
            inbound_rx,
            default_actor_id,
        }
    }

    pub async fn run(mut self) {
        let workspace_id = WorkspaceId::from_id("default");
        while let Some(message) = self.inbound_rx.recv().await {
            let payload = match message.input {
                PortInput::Chat { text } => MessagePayload::user_text(text),
                PortInput::Audio {
                    file_id,
                    mime_type,
                    duration_ms,
                    ..
                } => MessagePayload::UserAudio(borg_core::message_payload::UserAudioPayload {
                    file_id: file_id.to_string(),
                    transcript: None,
                    mime_type,
                    duration_ms,
                }),
                PortInput::Command(command) => MessagePayload::user_text(format!("/{:?}", command)),
            };

            // 1. Actor resolution happens on the port side
            let conversation_key = message.conversation_key.to_string();
            let actor_id = match self
                .rt
                .db
                .resolve_port_actor(&self.port_id, &conversation_key)
                .await
            {
                Ok(Some(id)) => id,
                Ok(None) => {
                    // Spawn or bind
                    let actor_id = self.default_actor_id.clone().unwrap_or_else(|| {
                        deterministic_actor_id(&self.port_id, &conversation_key)
                    });

                    // If deterministic, ensure actor exists
                    if self.default_actor_id.is_none() {
                        if let Ok(None) = self.rt.db.get_actor(&actor_id).await {
                            let _ = self
                                .rt
                                .db
                                .upsert_actor(
                                    &actor_id,
                                    &workspace_id,
                                    &format!("Port {} - {}", self.port_id, conversation_key),
                                    "You are a helpful assistant.",
                                    "",
                                    "RUNNING",
                                )
                                .await;
                        }
                    }

                    let _ = self
                        .rt
                        .db
                        .upsert_port_binding(
                            &workspace_id,
                            &self.port_id,
                            &conversation_key,
                            &actor_id,
                        )
                        .await;
                    actor_id
                }
                Err(err) => {
                    error!(target: "borg_ports", error = %err, "failed to resolve actor for port input");
                    continue;
                }
            };

            // 2. Send message to runtime
            if let Err(err) = self
                .rt
                .send_message(&self.port_id.clone().into(), &actor_id.into(), payload)
                .await
            {
                error!(target: "borg_ports", error = %err, "failed to send message from port to runtime");
            }
        }
    }
}

fn compute_reconcile_plan(
    current: &HashMap<PortId, RunningPortState>,
    desired: &HashMap<PortId, PortConfig>,
) -> Vec<ReconcileAction> {
    let current_port_set: HashSet<PortId> = current.keys().cloned().collect();
    let desired_port_set: HashSet<PortId> = desired.keys().cloned().collect();

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
        ReconcileAction::Stop(id) => ("0", id.to_string()),
        ReconcileAction::Restart(id) => ("1", id.to_string()),
        ReconcileAction::Start(id) => ("2", id.to_string()),
    }
}
