use std::sync::Arc;

use borg_core::Uri;
use borg_exec::PortContext;

#[derive(Debug, Clone)]
// TODO(@leostera): PortMessage<Data> and replace text with `data: Data`, so we can keep data typed
// here until it has to be rendered into the transport (socket, http request, etc)
pub struct PortMessage {
    pub port_id: Uri,
    pub conversation_key: Uri,
    pub user_id: Uri,
    pub text: String,
    pub port_context: Arc<dyn PortContext>,
}
