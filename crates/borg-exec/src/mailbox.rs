use borg_agent::{BorgToolCall, BorgToolResult, ContextWindow};
use borg_db::MessageRecord;
use tokio::sync::oneshot;

pub enum ActorCommand {
    /// Notify the actor that a new message has been delivered to its mailbox.
    Notify,
    /// Deliver a specific message to the actor.
    Message(MessageRecord),
    /// Request the current effective context window from the actor.
    InspectContext(oneshot::Sender<anyhow::Result<ContextWindow<BorgToolCall, BorgToolResult>>>),
    /// Terminate the actor loop.
    Terminate,
}
