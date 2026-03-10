//! Turn outcome types for actor execution.
//!
//! The runtime makes turn outcomes explicit in types rather than
//! relying on stringly-typed ad-hoc failures.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::ids::MessageId;

// ---------------------------------------------------------------------------
// Turn outcome
// ---------------------------------------------------------------------------

/// The result of processing one inbound mailbox message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TurnOutcome {
    /// The turn completed successfully and produced a final assistant message.
    Completed {
        /// The message_id of the final assistant message that closed the turn.
        final_message_id: MessageId,
    },
    /// The turn failed and the inbound message should be marked `failed`.
    Failed {
        /// Machine-readable failure code.
        code: TurnFailureCode,
        /// Human-readable failure detail.
        message: String,
    },
}

impl TurnOutcome {
    /// Returns `true` if the turn completed successfully.
    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }

    /// Returns `true` if the turn failed.
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    pub fn code(&self) -> TurnFailureCode {
        match self {
            Self::Completed { .. } => TurnFailureCode::RuntimeInvariantViolation, // Should not be called on Completed
            Self::Failed { code, .. } => *code,
        }
    }

    pub fn message(&self) -> Option<String> {
        match self {
            Self::Completed { .. } => None,
            Self::Failed { message, .. } => Some(message.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// Failure codes
// ---------------------------------------------------------------------------

/// Stable machine-readable failure codes for actor turn failures.
///
/// Tool failure is deliberately absent here -- a tool can fail and still
/// produce a valid structured tool result. That is not the same thing
/// as the actor turn failing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnFailureCode {
    /// The provider call could not be completed.
    ProviderFailure,
    /// A durable persistence operation failed.
    PersistenceFailure,
    /// The provider returned output that could not be parsed
    /// into a valid action list.
    InvalidProviderOutput,
    /// A tool execution could not be represented as a structured result.
    InvalidToolResult,
    /// An internal runtime invariant was violated.
    RuntimeInvariantViolation,
    /// The turn exceeded the maximum number of iterations.
    MaxTurnsExceeded,
}

impl TurnFailureCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProviderFailure => "provider_failure",
            Self::PersistenceFailure => "persistence_failure",
            Self::InvalidProviderOutput => "invalid_provider_output",
            Self::InvalidToolResult => "invalid_tool_result",
            Self::RuntimeInvariantViolation => "runtime_invariant_violation",
            Self::MaxTurnsExceeded => "max_turns_exceeded",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "provider_failure" => Some(Self::ProviderFailure),
            "persistence_failure" => Some(Self::PersistenceFailure),
            "invalid_provider_output" => Some(Self::InvalidProviderOutput),
            "invalid_tool_result" => Some(Self::InvalidToolResult),
            "runtime_invariant_violation" => Some(Self::RuntimeInvariantViolation),
            "max_turns_exceeded" => Some(Self::MaxTurnsExceeded),
            _ => None,
        }
    }
}

impl fmt::Display for TurnFailureCode {
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
    fn turn_outcome_completed() {
        let outcome = TurnOutcome::Completed {
            final_message_id: MessageId::from_id("msg-123"),
        };
        assert!(outcome.is_completed());
        assert!(!outcome.is_failed());
    }

    #[test]
    fn turn_outcome_failed() {
        let outcome = TurnOutcome::Failed {
            code: TurnFailureCode::ProviderFailure,
            message: "provider timeout".to_string(),
        };
        assert!(outcome.is_failed());
        assert!(!outcome.is_completed());
    }

    #[test]
    fn failure_code_roundtrip() {
        for code in [
            TurnFailureCode::ProviderFailure,
            TurnFailureCode::PersistenceFailure,
            TurnFailureCode::InvalidProviderOutput,
            TurnFailureCode::InvalidToolResult,
            TurnFailureCode::RuntimeInvariantViolation,
            TurnFailureCode::MaxTurnsExceeded,
        ] {
            let s = code.as_str();
            assert_eq!(TurnFailureCode::parse(s), Some(code));
        }
    }

    #[test]
    fn failure_code_serde_roundtrip() {
        let code = TurnFailureCode::ProviderFailure;
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"provider_failure\"");
        let parsed: TurnFailureCode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, code);
    }

    #[test]
    fn turn_outcome_serde_roundtrip() {
        let outcome = TurnOutcome::Failed {
            code: TurnFailureCode::PersistenceFailure,
            message: "db write failed".to_string(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        let parsed: TurnOutcome = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_failed());
    }
}
