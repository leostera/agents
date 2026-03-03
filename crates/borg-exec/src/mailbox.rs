use crate::message::{BorgMessage, SessionOutput};
use anyhow::Result;
use borg_agent::{BorgToolCall, BorgToolResult};
use borg_core::Uri;
use tokio::sync::oneshot;

pub enum ActorCommand {
    Cast {
        actor_message_id: Uri,
        msg: BorgMessage,
    },
    Call {
        actor_message_id: Uri,
        msg: BorgMessage,
        response_tx: oneshot::Sender<Result<SessionOutput<BorgToolCall, BorgToolResult>>>,
    },
    Terminate,
}
