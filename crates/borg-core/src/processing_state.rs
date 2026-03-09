//! Message processing state machine.
//!
//! Valid transitions:
//! - `pending -> processed`
//! - `pending -> failed`

use serde::{Deserialize, Serialize};
use std::fmt;

/// Durable processing state for a delivered message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProcessingState {
    /// Delivered and durable, but not yet marked complete.
    /// Replayed on actor startup.
    Pending,
    /// Turn completed successfully.
    Processed,
    /// Turn reached a terminal failure.
    Failed,
}

impl ProcessingState {
    /// Database string representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processed => "processed",
            Self::Failed => "failed",
        }
    }

    /// Parse from database string. Returns `None` for invalid values.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "processed" => Some(Self::Processed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    /// Parse from database string, returning an error for invalid values.
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        Self::from_str_opt(s).ok_or_else(|| {
            anyhow::anyhow!(
                "invalid processing state: `{s}` (expected pending, processed, or failed)"
            )
        })
    }

    /// Returns `true` if this is a terminal state (processed or failed).
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Processed | Self::Failed)
    }

    /// Returns `true` if this message should be replayed on actor startup.
    pub fn should_replay(self) -> bool {
        matches!(self, Self::Pending)
    }

    /// Validate a state transition. Returns `Err` if the transition is invalid.
    pub fn transition_to(self, target: Self) -> anyhow::Result<Self> {
        match (self, target) {
            (Self::Pending, Self::Processed) => Ok(Self::Processed),
            (Self::Pending, Self::Failed) => Ok(Self::Failed),
            (from, to) => Err(anyhow::anyhow!(
                "invalid message state transition: {} -> {}",
                from,
                to,
            )),
        }
    }
}

impl fmt::Display for ProcessingState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Tool call status
// ---------------------------------------------------------------------------

/// Status of a tool call during execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolCallStatus {
    /// Tool call started but not yet finished.
    Running,
    /// Tool call completed successfully.
    Succeeded,
    /// Tool call completed with an error (but the turn may continue).
    Failed,
}

impl ToolCallStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> anyhow::Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            _ => Err(anyhow::anyhow!(
                "invalid tool call status: `{s}` (expected running, succeeded, or failed)"
            )),
        }
    }
}

impl fmt::Display for ToolCallStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_transitions() {
        assert_eq!(
            ProcessingState::Pending
                .transition_to(ProcessingState::Processed)
                .unwrap(),
            ProcessingState::Processed,
        );
        assert_eq!(
            ProcessingState::Pending
                .transition_to(ProcessingState::Failed)
                .unwrap(),
            ProcessingState::Failed,
        );
    }

    #[test]
    fn invalid_transitions() {
        assert!(
            ProcessingState::Processed
                .transition_to(ProcessingState::Failed)
                .is_err()
        );
        assert!(
            ProcessingState::Failed
                .transition_to(ProcessingState::Processed)
                .is_err()
        );
        assert!(
            ProcessingState::Processed
                .transition_to(ProcessingState::Pending)
                .is_err()
        );
        assert!(
            ProcessingState::Failed
                .transition_to(ProcessingState::Pending)
                .is_err()
        );
        // Self-transitions are also invalid
        assert!(
            ProcessingState::Pending
                .transition_to(ProcessingState::Pending)
                .is_err()
        );
    }

    #[test]
    fn parse_roundtrip() {
        for state in [
            ProcessingState::Pending,
            ProcessingState::Processed,
            ProcessingState::Failed,
        ] {
            assert_eq!(ProcessingState::parse(state.as_str()).unwrap(), state);
        }
    }

    #[test]
    fn parse_case_insensitive() {
        assert_eq!(
            ProcessingState::parse("PENDING").unwrap(),
            ProcessingState::Pending,
        );
        assert_eq!(
            ProcessingState::parse("Processed").unwrap(),
            ProcessingState::Processed,
        );
    }

    #[test]
    fn parse_invalid() {
        assert!(ProcessingState::parse("queued").is_err());
        assert!(ProcessingState::parse("").is_err());
    }

    #[test]
    fn terminal_states() {
        assert!(!ProcessingState::Pending.is_terminal());
        assert!(ProcessingState::Processed.is_terminal());
        assert!(ProcessingState::Failed.is_terminal());
    }

    #[test]
    fn replay_states() {
        assert!(ProcessingState::Pending.should_replay());
        assert!(!ProcessingState::Processed.should_replay());
        assert!(!ProcessingState::Failed.should_replay());
    }

    #[test]
    fn tool_call_status_parse_roundtrip() {
        for status in [
            ToolCallStatus::Running,
            ToolCallStatus::Succeeded,
            ToolCallStatus::Failed,
        ] {
            assert_eq!(ToolCallStatus::parse(status.as_str()).unwrap(), status);
        }
    }

    #[test]
    fn serde_roundtrip() {
        let state = ProcessingState::Pending;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"pending\"");
        let parsed: ProcessingState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, state);
    }
}
