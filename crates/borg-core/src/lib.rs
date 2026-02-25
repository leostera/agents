pub mod borgdir;
mod entity;
mod event;
mod execution;
mod task;
mod uri;

pub use entity::Entity;
pub use event::Event;
pub use execution::ExecutionResult;
pub use task::{Task, TaskEvent, TaskKind, TaskStatus};
pub use uri::Uri;

#[macro_export]
macro_rules! uri {
    ($ns:expr, $kind:expr) => {
        $crate::Uri::from_parts($ns, $kind, None).unwrap()
    };
    ($ns:expr, $kind:expr, $id:expr) => {
        $crate::Uri::from_parts($ns, $kind, Some($id)).unwrap()
    };
}
