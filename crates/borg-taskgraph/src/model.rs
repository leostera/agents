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
    pub data: TaskEventData,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum TaskEventData {
    Empty {},
    TaskCreated {
        assignee_agent_id: String,
        assignee_session_uri: String,
        reviewer_agent_id: String,
        reviewer_session_uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_uri: Option<String>,
    },
    TaskUpdated {
        title: String,
        description: String,
        definition_of_done: String,
    },
    TaskReassigned {
        old_assignee_agent_id: String,
        old_assignee_session_uri: String,
        new_assignee_agent_id: String,
        new_assignee_session_uri: String,
    },
    Labels {
        labels: Vec<String>,
    },
    ParentSet {
        parent_uri: String,
    },
    BlockedBy {
        blocked_by: String,
    },
    DuplicateOf {
        duplicate_of: String,
    },
    Reference {
        reference: String,
    },
    Status {
        status: String,
    },
    ReviewSubmitted {
        submitted_at: String,
    },
    ReviewApproved {
        approved_at: String,
    },
    ReviewChangesRequested {
        changes_requested_at: String,
        return_to: String,
        note: String,
    },
    TaskSplit {
        subtask_count: i64,
    },
    CommentAdded {
        comment_id: String,
    },
}

impl Default for TaskEventData {
    fn default() -> Self {
        Self::Empty {}
    }
}
