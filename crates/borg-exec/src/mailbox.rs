use crate::message::{ActorOutput, BorgMessage};
use anyhow::Result;
use borg_agent::{BorgToolCall, BorgToolResult};
use borg_core::Uri;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;

pub enum ActorCommand {
    Cast {
        actor_message_id: Uri,
        sender_actor_id: Option<Uri>,
        msg: BorgMessage,
    },
    Call {
        actor_message_id: Uri,
        sender_actor_id: Option<Uri>,
        msg: BorgMessage,
        progress_tx: Option<Sender<ActorOutput<BorgToolCall, BorgToolResult>>>,
        response_tx: oneshot::Sender<Result<ActorOutput<BorgToolCall, BorgToolResult>>>,
    },
    Terminate,
}
