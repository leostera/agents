use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Doing,
    Review,
    Done,
    Discarded,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Doing => "doing",
            Self::Review => "review",
            Self::Done => "done",
            Self::Discarded => "discarded",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "pending" => Some(Self::Pending),
            "doing" => Some(Self::Doing),
            "review" => Some(Self::Review),
            "done" => Some(Self::Done),
            "discarded" => Some(Self::Discarded),
            _ => None,
        }
    }

    pub fn is_complete(self) -> bool {
        matches!(self, Self::Done | Self::Discarded)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewState {
    pub submitted_at: Option<String>,
    pub approved_at: Option<String>,
    pub changes_requested_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub uri: String,
    pub title: String,
    pub description: String,
    pub definition_of_done: String,
    pub status: String,
    pub assignee_agent_id: String,
    pub assignee_session_uri: String,
    pub reviewer_agent_id: String,
    pub reviewer_session_uri: String,
    pub labels: Vec<String>,
    pub parent_uri: Option<String>,
    pub blocked_by: Vec<String>,
    pub duplicate_of: Option<String>,
    pub references: Vec<String>,
    pub review: ReviewState,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentRecord {
    pub id: String,
    pub task_uri: String,
    pub author_session_uri: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: String,
    pub task_uri: String,
    pub actor_session_uri: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: serde_json::Value,
    pub created_at: String,
}
