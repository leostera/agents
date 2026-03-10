use borg_core::Uri;
use borg_exec::{BorgCommand, PortContext};

#[derive(Debug, Clone)]
pub enum PortInput {
    Chat {
        text: String,
    },
    Audio {
        file_id: Uri,
        mime_type: Option<String>,
        duration_ms: Option<u64>,
        language_hint: Option<String>,
    },
    Command(BorgCommand),
}

#[derive(Debug, Clone)]
pub struct PortMessage {
    pub port_id: Uri,
    pub conversation_key: Uri,
    pub user_id: Uri,
    pub input: PortInput,
    pub port_context: PortContext,
}

impl PortMessage {
    pub fn from_text(
        port_id: Uri,
        conversation_key: Uri,
        user_id: Uri,
        text: String,
        port_context: PortContext,
    ) -> Self {
        Self {
            port_id,
            conversation_key,
            user_id,
            input: PortInput::Chat { text },
            port_context,
        }
    }
}
