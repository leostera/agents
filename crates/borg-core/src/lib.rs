pub mod borgdir;
pub mod config;
mod entity;
mod event;
mod execution;
mod telegram;
mod uri;

// RFD0033 canonical runtime types
pub mod action;
pub mod ids;
pub mod message_payload;
pub mod processing_state;
pub mod turn;

pub use config::Config;
pub use entity::{Entity, EntityPropValue, EntityProps};
pub use event::{ActorContextSnapshot, ActorToolSchema, Event};
pub use execution::ExecutionResult;
pub use telegram::TelegramUserId;
pub use uri::Uri;

// Re-export RFD0033 types at crate root for convenience
pub use action::{
    Action, ActionList, FinalAssistantMessageAction, OutboundMessageAction, ToolCallAction,
};
pub use ids::{
    ActorId, CorrelationId, EndpointUri, LlmCallId, MessageId, PortId, ProviderId, ToolCallId,
    WorkspaceId,
};
pub use message_payload::MessagePayload;
pub use processing_state::{ProcessingState, ToolCallStatus};
pub use turn::{TurnFailureCode, TurnOutcome};

#[macro_export]
macro_rules! uri {
    ($ns:expr, $kind:expr) => {
        $crate::Uri::from_parts($ns, $kind, Some(&::uuid::Uuid::now_v7().to_string())).unwrap()
    };
    ($ns:expr, $kind:expr, $id:expr) => {
        $crate::Uri::from_parts($ns, $kind, Some($id)).unwrap()
    };
}
