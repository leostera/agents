mod actor;
mod actor_context_manager;
mod actor_manager;
mod llm_resolver;
mod mailbox;
mod mailbox_envelope;
mod message;
mod runtime;

pub use actor_manager::BorgActorManager;
pub use message::{
    ActorOutboundMessage, ActorOutput, BorgCommand, BorgInput, BorgMessage,
    PortInboundActorMessage, RuntimeToolCall, RuntimeToolResult,
};
pub use port_context::{DiscordContext, HttpContext, PortContext, TelegramContext};
pub use runtime::BorgRuntime;

mod patch_apply;
mod port_context;
mod provider_config;
mod provider_supervisor;
pub mod tool_contract;
mod tool_runner;
mod types;

pub use borg_llm::ReasoningEffort;
pub use provider_config::ProviderConfigSnapshot;
pub use provider_supervisor::ProviderConfigSupervisor;
pub use types::{ActorTurnOutput, ToolCallSummary};
