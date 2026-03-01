use std::sync::Arc;

use borg_core::Uri;
use borg_exec::{BorgCommand, PortContext};

#[derive(Debug, Clone)]
pub enum PortInput {
    Chat { text: String },
    Command(BorgCommand),
}

#[derive(Debug, Clone)]
// TODO(@leostera): PortMessage<Data> and replace text with `data: Data`, so we can keep data typed
// here until it has to be rendered into the transport (socket, http request, etc)
pub struct PortMessage {
    pub port_id: Uri,
    pub conversation_key: Uri,
    pub user_id: Uri,
    pub input: PortInput,
    pub port_context: Arc<dyn PortContext>,
}
