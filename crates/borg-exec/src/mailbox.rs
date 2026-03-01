use crate::message::{BorgMessage, SessionOutput};
use anyhow::Result;
use tokio::sync::oneshot;

pub enum ActorCommand {
    Cast(BorgMessage),
    Call(BorgMessage, oneshot::Sender<Result<SessionOutput>>),
    Terminate,
}
