pub mod borgdir;
pub mod config;
mod entity;
mod event;
mod execution;
mod telegram;
mod uri;

pub use config::Config;
pub use entity::{Entity, EntityPropValue, EntityProps};
pub use event::{Event, SessionContextSnapshot, SessionToolSchema};
pub use execution::ExecutionResult;
pub use telegram::TelegramUserId;
pub use uri::Uri;

#[macro_export]
macro_rules! uri {
    ($ns:expr, $kind:expr) => {
        $crate::Uri::from_parts($ns, $kind, Some(&::uuid::Uuid::now_v7().to_string())).unwrap()
    };
    ($ns:expr, $kind:expr, $id:expr) => {
        $crate::Uri::from_parts($ns, $kind, Some($id)).unwrap()
    };
}
