use crate::message::{BorgMessage, SessionOutput};
use anyhow::Result;
use borg_agent::{BorgToolCall, BorgToolResult};
use borg_core::Uri;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;

pub enum ActorCommand {
    Cast {
        actor_message_id: Uri,
        msg: BorgMessage,
    },
    Call {
        actor_message_id: Uri,
        msg: BorgMessage,
        progress_tx: Option<Sender<SessionOutput<BorgToolCall, BorgToolResult>>>,
        response_tx: oneshot::Sender<Result<SessionOutput<BorgToolCall, BorgToolResult>>>,
    },
    Terminate,
}
