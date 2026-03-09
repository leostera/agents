//! Ordered action list for structured provider output.
//!
//! Provider output is represented internally as a single ordered list of
//! actions. The runtime processes actions in sequence -- it must never
//! normalize a response into separate buckets that reorder side effects.

use serde::{Deserialize, Serialize};

use crate::ids::{EndpointUri, ToolCallId};

// ---------------------------------------------------------------------------
// Action list
// ---------------------------------------------------------------------------

/// An ordered list of actions produced by a provider response.
///
/// Execution always processes actions in sequence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionList(pub Vec<Action>);

impl ActionList {
    /// Create an empty action list.
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// Create an action list from a vec of actions.
    pub fn new(actions: Vec<Action>) -> Self {
        Self(actions)
    }

    /// Returns `true` if the list contains a `FinalAssistantMessage`.
    pub fn has_final_message(&self) -> bool {
        self.0
            .iter()
            .any(|a| matches!(a, Action::FinalAssistantMessage(_)))
    }

    /// Returns the number of actions.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate over actions.
    pub fn iter(&self) -> impl Iterator<Item = &Action> {
        self.0.iter()
    }

    /// Consume and iterate over actions.
    pub fn into_iter(self) -> impl Iterator<Item = Action> {
        self.0.into_iter()
    }
}

impl IntoIterator for ActionList {
    type Item = Action;
    type IntoIter = std::vec::IntoIter<Action>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a ActionList {
    type Item = &'a Action;
    type IntoIter = std::slice::Iter<'a, Action>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

// ---------------------------------------------------------------------------
// Action variants
// ---------------------------------------------------------------------------

/// A single action emitted by provider output during a turn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum Action {
    /// Invoke a tool and feed the result back into the turn loop.
    #[serde(rename = "tool_call")]
    ToolCall(ToolCallAction),

    /// Send a message to another actor or to a port.
    #[serde(rename = "message")]
    Message(OutboundMessageAction),

    /// Terminate the current turn with a final assistant message.
    #[serde(rename = "final_assistant_message")]
    FinalAssistantMessage(FinalAssistantMessageAction),
}

// ---------------------------------------------------------------------------
// Action payloads
// ---------------------------------------------------------------------------

/// A tool call action requesting tool execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallAction {
    pub tool_call_id: ToolCallId,
    pub tool_name: String,
    pub input_json: String,
}

/// An outbound message action (actor->actor or actor->port).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutboundMessageAction {
    pub receiver_id: EndpointUri,
    pub payload_json: String,
}

/// A final assistant message action that closes the current turn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FinalAssistantMessageAction {
    pub text: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_list_roundtrip() {
        let actions = ActionList::new(vec![
            Action::ToolCall(ToolCallAction {
                tool_call_id: ToolCallId::from_id("call_001"),
                tool_name: "Patch-apply".to_string(),
                input_json: r#"{"patch":"..."}"#.to_string(),
            }),
            Action::Message(OutboundMessageAction {
                receiver_id: EndpointUri::parse("borg:actor:planner").unwrap(),
                payload_json: r#"{"kind":"task_update","task":"..."}"#.to_string(),
            }),
            Action::Message(OutboundMessageAction {
                receiver_id: EndpointUri::parse("borg:port:telegram-main").unwrap(),
                payload_json: r#"{"kind":"text","text":"done"}"#.to_string(),
            }),
            Action::FinalAssistantMessage(FinalAssistantMessageAction {
                text: "all set".to_string(),
            }),
        ]);

        let json = serde_json::to_string(&actions).unwrap();
        let parsed: ActionList = serde_json::from_str(&json).unwrap();
        assert_eq!(actions, parsed);
    }

    #[test]
    fn has_final_message_true_when_present() {
        let actions = ActionList::new(vec![Action::FinalAssistantMessage(
            FinalAssistantMessageAction {
                text: "done".to_string(),
            },
        )]);
        assert!(actions.has_final_message());
    }

    #[test]
    fn has_final_message_false_when_absent() {
        let actions = ActionList::new(vec![Action::ToolCall(ToolCallAction {
            tool_call_id: ToolCallId::from_id("c1"),
            tool_name: "test".to_string(),
            input_json: "{}".to_string(),
        })]);
        assert!(!actions.has_final_message());
    }

    #[test]
    fn empty_action_list() {
        let actions = ActionList::empty();
        assert!(actions.is_empty());
        assert_eq!(actions.len(), 0);
        assert!(!actions.has_final_message());
    }

    #[test]
    fn action_list_ordering_preserved() {
        let actions = ActionList::new(vec![
            Action::ToolCall(ToolCallAction {
                tool_call_id: ToolCallId::from_id("c1"),
                tool_name: "first".to_string(),
                input_json: "{}".to_string(),
            }),
            Action::FinalAssistantMessage(FinalAssistantMessageAction {
                text: "last".to_string(),
            }),
        ]);

        let collected: Vec<_> = actions.iter().collect();
        assert_eq!(collected.len(), 2);
        assert!(matches!(collected[0], Action::ToolCall(_)));
        assert!(matches!(collected[1], Action::FinalAssistantMessage(_)));
    }
}
