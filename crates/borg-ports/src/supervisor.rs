use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Result;
use borg_db::BorgDb;
use borg_exec::ExecEngine;
use tokio::{sync::Mutex, task::JoinHandle, time};
use tracing::{error, info, warn};

use crate::TelegramPort;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(3);
const PORT_KIND_TELEGRAM: &str = "telegram";
const PORT_KIND_KEY: &str = "kind";
const PORT_ENABLED_KEY: &str = "enabled";
const TELEGRAM_BOT_TOKEN_KEY: &str = "bot_token";
const PORT_SCAN_LIMIT: usize = 1_000;

type PortName = String;

struct TelegramRuntime {
    token: String,
    task: JoinHandle<()>,
}

#[derive(Default)]
struct SupervisorState {
    telegram: HashMap<PortName, TelegramRuntime>,
}

#[derive(Clone)]
pub struct BorgPortsSupervisor {
    db: BorgDb,
    exec: ExecEngine,
    state: Arc<Mutex<SupervisorState>>,
    poll_interval: Duration,
    enabled: bool,
}

impl BorgPortsSupervisor {
    pub fn new(db: BorgDb, exec: ExecEngine) -> Self {
        Self {
            db,
            exec,
            state: Arc::new(Mutex::new(SupervisorState::default())),
            poll_interval: DEFAULT_POLL_INTERVAL,
            enabled: true,
        }
    }

    pub fn disabled(db: BorgDb, exec: ExecEngine) -> Self {
        Self {
            db,
            exec,
            state: Arc::new(Mutex::new(SupervisorState::default())),
            poll_interval: DEFAULT_POLL_INTERVAL,
            enabled: false,
        }
    }

    pub fn with_poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    pub fn start(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            if !self.enabled {
                return;
            }

            if let Err(err) = self.reconcile_now().await {
                error!(target: "borg_ports", error = %err, "ports supervisor initial reconcile failed");
            }

            let mut ticker = time::interval(self.poll_interval);
            loop {
                ticker.tick().await;
                if let Err(err) = self.reconcile_now().await {
                    error!(target: "borg_ports", error = %err, "ports supervisor reconcile failed");
                }
            }
        })
    }

    pub async fn reconcile_now(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        self.reconcile_telegram_ports().await
    }

    pub async fn on_port_setting_changed(&self, _port: &str, key: &str) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if matches!(
            key,
            PORT_KIND_KEY | PORT_ENABLED_KEY | TELEGRAM_BOT_TOKEN_KEY
        ) {
            self.reconcile_telegram_ports().await?;
        }
        Ok(())
    }

    pub async fn shutdown(&self) {
        let mut state = self.state.lock().await;
        for (_, runtime) in state.telegram.drain() {
            runtime.task.abort();
        }
    }

    async fn reconcile_telegram_ports(&self) -> Result<()> {
        let desired_ports = self.resolve_desired_telegram_ports().await?;
        let mut state = self.state.lock().await;

        let finished_ports: Vec<String> = state
            .telegram
            .iter()
            .filter_map(|(port, runtime)| runtime.task.is_finished().then_some(port.clone()))
            .collect();
        for port in finished_ports {
            warn!(target: "borg_ports", port = %port, "telegram port task exited; restarting if configured");
            state.telegram.remove(&port);
        }

        let running_ports: Vec<String> = state.telegram.keys().cloned().collect();
        for port in running_ports {
            let Some(desired_token) = desired_ports.get(&port) else {
                if let Some(runtime) = state.telegram.remove(&port) {
                    runtime.task.abort();
                    info!(target: "borg_ports", port = %port, "telegram port disabled");
                }
                continue;
            };

            let should_restart = state
                .telegram
                .get(&port)
                .is_some_and(|runtime| runtime.token != *desired_token);
            if should_restart && let Some(runtime) = state.telegram.remove(&port) {
                runtime.task.abort();
                info!(target: "borg_ports", port = %port, "telegram port token changed, restarting");
            }
        }

        for (port, token) in desired_ports {
            if state.telegram.contains_key(&port) {
                continue;
            }

            let telegram_port = TelegramPort::new(self.exec.clone(), port.clone(), token.clone())?;
            let task = tokio::spawn(async move {
                if let Err(err) = telegram_port.run().await {
                    error!(target: "borg_ports", error = %err, "telegram port terminated");
                }
            });
            state
                .telegram
                .insert(port.clone(), TelegramRuntime { token, task });
            info!(target: "borg_ports", port = %port, "telegram port enabled");
        }

        Ok(())
    }

    async fn resolve_desired_telegram_ports(&self) -> Result<HashMap<String, String>> {
        let mut desired = HashMap::new();
        let ports = self.db.list_ports(PORT_SCAN_LIMIT).await?;

        for port_record in ports {
            let port = port_record.port;
            let kind = self
                .db
                .get_port_setting(&port, PORT_KIND_KEY)
                .await?
                .map(|value| value.trim().to_ascii_lowercase());

            let is_telegram_kind = kind
                .as_deref()
                .is_some_and(|value| value == PORT_KIND_TELEGRAM)
                || (kind.is_none() && port == PORT_KIND_TELEGRAM);
            if !is_telegram_kind {
                continue;
            }

            let enabled = self
                .db
                .get_port_setting(&port, PORT_ENABLED_KEY)
                .await?
                .is_none_or(|raw| parse_enabled(&raw));
            if !enabled {
                continue;
            }

            let token = self
                .db
                .get_port_setting(&port, TELEGRAM_BOT_TOKEN_KEY)
                .await?
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            if let Some(token) = token {
                desired.insert(port, token);
            }
        }

        Ok(desired)
    }
}

fn parse_enabled(raw: &str) -> bool {
    let normalized = raw.trim().to_ascii_lowercase();
    !matches!(normalized.as_str(), "0" | "false" | "no" | "off")
}
