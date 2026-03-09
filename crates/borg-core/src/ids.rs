//! Strong ID newtypes for Borg runtime entities.
//!
//! These wrappers prevent accidental misuse of raw strings where
//! a specific identifier kind is required.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::Uri;

// ---------------------------------------------------------------------------
// Macro to reduce boilerplate for URI-backed IDs
// ---------------------------------------------------------------------------

macro_rules! uri_id {
    ($(#[$meta:meta])* $name:ident, $scheme:expr, $kind:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(pub Uri);

        impl $name {
            /// Create a new unique identifier using UUIDv7.
            pub fn new() -> Self {
                Self(Uri::from_parts($scheme, $kind, Some(&uuid::Uuid::now_v7().to_string())).unwrap())
            }

            /// Create an identifier from a specific string ID.
            pub fn from_id(id: &str) -> Self {
                Self(Uri::from_parts($scheme, $kind, Some(id)).unwrap())
            }

            pub fn parse(input: &str) -> anyhow::Result<Self> {
                Uri::parse(input).map(Self)
            }

            pub fn as_uri(&self) -> &Uri {
                &self.0
            }

            pub fn into_uri(self) -> Uri {
                self.0
            }

            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<Uri> for $name {
            fn from(uri: Uri) -> Self {
                Self(uri)
            }
        }

        impl From<$name> for Uri {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                self.0.serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Uri::deserialize(deserializer).map(Self)
            }
        }
    };
}

// ---------------------------------------------------------------------------
// URI-backed identifiers (endpoint-addressable entities)
// ---------------------------------------------------------------------------

uri_id! {
    /// Stable endpoint URI for an actor. Format: `borg:actor:<id>`.
    ActorId, "borg", "actor"
}

uri_id! {
    /// Stable endpoint URI for a port. Format: `borg:port:<name>`.
    PortId, "borg", "port"
}

uri_id! {
    /// Workspace identifier.
    WorkspaceId, "borg", "workspace"
}

uri_id! {
    /// Unique message identifier.
    MessageId, "borg", "message"
}

uri_id! {
    /// Unique tool call identifier.
    ToolCallId, "borg", "tool-call"
}

uri_id! {
    /// Unique LLM call identifier.
    LlmCallId, "borg", "llm-call"
}

uri_id! {
    /// Provider identifier.
    ProviderId, "borg", "provider"
}

uri_id! {
    /// Correlation identifier spanning all messages, tool calls, and LLM calls
    /// caused by processing one inbound message.
    CorrelationId, "borg", "correlation"
}

/// Opaque URI used as a message sender or receiver.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EndpointUri(pub Uri);

impl EndpointUri {
    pub fn parse(input: &str) -> anyhow::Result<Self> {
        Uri::parse(input).map(Self)
    }

    pub fn as_uri(&self) -> &Uri {
        &self.0
    }

    pub fn into_uri(self) -> Uri {
        self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<Uri> for EndpointUri {
    fn from(uri: Uri) -> Self {
        Self(uri)
    }
}

impl From<EndpointUri> for Uri {
    fn from(id: EndpointUri) -> Self {
        id.0
    }
}

impl From<EndpointUri> for String {
    fn from(id: EndpointUri) -> Self {
        id.0.to_string()
    }
}

impl From<ActorId> for EndpointUri {
    fn from(id: ActorId) -> Self {
        Self(id.0)
    }
}

impl From<PortId> for EndpointUri {
    fn from(id: PortId) -> Self {
        Self(id.0)
    }
}

impl fmt::Display for EndpointUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_id_roundtrip() {
        let id = ActorId::new();
        assert!(id.as_str().starts_with("borg:actor:"));

        let json = serde_json::to_string(&id).unwrap();
        let parsed: ActorId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn message_id_generate_is_unique() {
        let a = MessageId::new();
        let b = MessageId::new();
        assert_ne!(a, b);
        assert!(a.as_str().starts_with("borg:message:"));
    }

    #[test]
    fn endpoint_uri_from_actor_id() {
        let actor = ActorId::parse("borg:actor:test").unwrap();
        let endpoint: EndpointUri = actor.into();
        assert_eq!(endpoint.as_str(), "borg:actor:test");
    }
}
