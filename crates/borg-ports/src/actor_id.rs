use borg_core::{ActorId, PortId};

/// Computes a deterministic actor ID from a port ID and a conversation key.
/// The resulting URI follows the format: `borg:actor:port/<port-name>/<conversation-key>`
pub fn deterministic_actor_id(port_id: &PortId, conversation_key: &str) -> ActorId {
    let port_name = port_id.as_str().split(':').last().unwrap_or("unknown");
    let id = format!("port/{}/{}", port_name, conversation_key);
    ActorId::from_id(&id)
}
