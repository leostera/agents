use std::str::FromStr;

use anyhow::Result;
use async_trait::async_trait;
use borg_core::Uri;
use borg_exec::{RuntimeToolCall, RuntimeToolResult, SessionOutput};
use tokio::sync::mpsc::{Receiver, Sender};

use crate::PortMessage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Privacy {
    Private,
    Public,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    Discord,
    Telegram,
    Unknown,
}

impl FromStr for Provider {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "discord" => Ok(Provider::Discord),
            "telegram" => Ok(Provider::Telegram),
            _ => Ok(Provider::Unknown),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortConfig {
    pub port_id: Uri,
    pub port_name: String,
    pub provider: Provider,
    pub status: Status,
    pub privacy: Privacy,
    pub assigned_actor_id: Option<Uri>,
    pub settings_json: String,
}

impl PortConfig {
    pub fn from_record(port_record: borg_db::PortRecord) -> Result<Self> {
        Ok(Self {
            port_id: port_record.port_id,
            port_name: port_record.port_name,
            provider: port_record.provider.parse()?,
            status: if port_record.enabled {
                Status::Enabled
            } else {
                Status::Disabled
            },
            privacy: if port_record.allows_guests {
                Privacy::Public
            } else {
                Privacy::Private
            },
            assigned_actor_id: port_record.assigned_actor_id,
            settings_json: port_record.settings.to_string(),
        })
    }
}

#[async_trait]
pub trait Port: Send + Sync + Sized + 'static {
    async fn new(config: PortConfig) -> Result<Self>;

    async fn run(
        self,
        inbound: Sender<PortMessage>,
        outbound: Receiver<SessionOutput<RuntimeToolCall, RuntimeToolResult>>,
    ) -> Result<()>;
}
