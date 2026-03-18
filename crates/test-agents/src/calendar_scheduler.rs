use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use borg_agent::{
    Agent, AgentEvent, AgentInput, CallbackToolRunner, ContextManager, ExecutionProfile,
    ToolCallEnvelope, ToolExecutionResult, ToolResultEnvelope,
};
use borg_evals_core::prelude::*;
use borg_llm::completion::{InputItem, ModelSelector, Temperature, TokenLimit};
use borg_llm::error::{Error as LlmError, LlmResult};
use borg_llm::runner::LlmRunner;
use borg_llm::testing::{
    TestContext, TestProvider, optional_test_env, runner_with_anthropic_model,
    runner_with_openai_model, runner_with_openrouter_model,
};
use borg_llm::tools::{RawToolDefinition, TypedTool};
use borg_macros::Tool;
use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CalendarEvent {
    event_id: String,
    title: String,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    locked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CalendarFixture {
    working_day_start: DateTime<Utc>,
    working_day_end: DateTime<Utc>,
    events: Vec<CalendarEvent>,
}

impl CalendarFixture {
    fn scattered_day() -> Self {
        Self {
            working_day_start: at_minute(9 * 60),
            working_day_end: at_minute(17 * 60),
            events: vec![
                CalendarEvent {
                    event_id: "evt-alex-1-1".to_string(),
                    title: "1:1 with Alex".to_string(),
                    start_at: at_minute(9 * 60),
                    end_at: at_minute(9 * 60 + 30),
                    locked: false,
                },
                CalendarEvent {
                    event_id: "evt-design-review".to_string(),
                    title: "Design review".to_string(),
                    start_at: at_minute(11 * 60),
                    end_at: at_minute(12 * 60),
                    locked: false,
                },
                CalendarEvent {
                    event_id: "evt-hiring-sync".to_string(),
                    title: "Hiring sync".to_string(),
                    start_at: at_minute(15 * 60),
                    end_at: at_minute(15 * 60 + 30),
                    locked: true,
                },
            ],
        }
    }

    fn fully_booked() -> Self {
        Self {
            working_day_start: at_minute(9 * 60),
            working_day_end: at_minute(17 * 60),
            events: vec![
                CalendarEvent {
                    event_id: "evt-planning".to_string(),
                    title: "Planning".to_string(),
                    start_at: at_minute(9 * 60),
                    end_at: at_minute(10 * 60),
                    locked: true,
                },
                CalendarEvent {
                    event_id: "evt-design".to_string(),
                    title: "Design".to_string(),
                    start_at: at_minute(10 * 60),
                    end_at: at_minute(12 * 60),
                    locked: true,
                },
                CalendarEvent {
                    event_id: "evt-lunch-1-1s".to_string(),
                    title: "Lunch and 1:1s".to_string(),
                    start_at: at_minute(12 * 60),
                    end_at: at_minute(15 * 60),
                    locked: true,
                },
                CalendarEvent {
                    event_id: "evt-recruiting".to_string(),
                    title: "Recruiting".to_string(),
                    start_at: at_minute(15 * 60),
                    end_at: at_minute(17 * 60),
                    locked: true,
                },
            ],
        }
    }

    fn anchored_day() -> Self {
        Self {
            working_day_start: at_minute(9 * 60),
            working_day_end: at_minute(18 * 60),
            events: vec![
                CalendarEvent {
                    event_id: "evt-staff-sync".to_string(),
                    title: "Staff sync".to_string(),
                    start_at: at_minute(9 * 60),
                    end_at: at_minute(9 * 60 + 30),
                    locked: false,
                },
                CalendarEvent {
                    event_id: "evt-customer-follow-up".to_string(),
                    title: "Customer follow-up".to_string(),
                    start_at: at_minute(11 * 60),
                    end_at: at_minute(11 * 60 + 30),
                    locked: false,
                },
                CalendarEvent {
                    event_id: "evt-architecture-review".to_string(),
                    title: "Architecture review".to_string(),
                    start_at: at_minute(14 * 60),
                    end_at: at_minute(15 * 60),
                    locked: true,
                },
                CalendarEvent {
                    event_id: "evt-mentoring".to_string(),
                    title: "Mentoring".to_string(),
                    start_at: at_minute(16 * 60 + 30),
                    end_at: at_minute(17 * 60),
                    locked: false,
                },
            ],
        }
    }

    fn already_good_day() -> Self {
        Self {
            working_day_start: at_minute(9 * 60),
            working_day_end: at_minute(17 * 60),
            events: vec![
                CalendarEvent {
                    event_id: "evt-daily-sync".to_string(),
                    title: "Daily sync".to_string(),
                    start_at: at_minute(9 * 60),
                    end_at: at_minute(9 * 60 + 30),
                    locked: true,
                },
                CalendarEvent {
                    event_id: "evt-planning".to_string(),
                    title: "Planning".to_string(),
                    start_at: at_minute(10 * 60),
                    end_at: at_minute(11 * 60),
                    locked: false,
                },
                CalendarEvent {
                    event_id: "evt-weekly-review".to_string(),
                    title: "Weekly review".to_string(),
                    start_at: at_minute(15 * 60 + 30),
                    end_at: at_minute(16 * 60),
                    locked: true,
                },
            ],
        }
    }

    fn dense_mixed_day() -> Self {
        Self {
            working_day_start: at_minute(9 * 60),
            working_day_end: at_minute(18 * 60),
            events: vec![
                CalendarEvent {
                    event_id: "evt-standup".to_string(),
                    title: "Standup".to_string(),
                    start_at: at_minute(9 * 60 + 30),
                    end_at: at_minute(10 * 60),
                    locked: false,
                },
                CalendarEvent {
                    event_id: "evt-design-pairing".to_string(),
                    title: "Design pairing".to_string(),
                    start_at: at_minute(10 * 60 + 30),
                    end_at: at_minute(11 * 60),
                    locked: false,
                },
                CalendarEvent {
                    event_id: "evt-lunch".to_string(),
                    title: "Lunch".to_string(),
                    start_at: at_minute(12 * 60),
                    end_at: at_minute(13 * 60),
                    locked: true,
                },
                CalendarEvent {
                    event_id: "evt-project-sync".to_string(),
                    title: "Project sync".to_string(),
                    start_at: at_minute(13 * 60 + 30),
                    end_at: at_minute(14 * 60),
                    locked: false,
                },
                CalendarEvent {
                    event_id: "evt-customer-review".to_string(),
                    title: "Customer review".to_string(),
                    start_at: at_minute(16 * 60),
                    end_at: at_minute(16 * 60 + 30),
                    locked: true,
                },
                CalendarEvent {
                    event_id: "evt-interview-debrief".to_string(),
                    title: "Interview debrief".to_string(),
                    start_at: at_minute(17 * 60),
                    end_at: at_minute(17 * 60 + 30),
                    locked: false,
                },
            ],
        }
    }

    fn constrained_partial_day() -> Self {
        Self {
            working_day_start: at_minute(9 * 60),
            working_day_end: at_minute(17 * 60),
            events: vec![
                CalendarEvent {
                    event_id: "evt-executive-sync".to_string(),
                    title: "Executive sync".to_string(),
                    start_at: at_minute(9 * 60),
                    end_at: at_minute(10 * 60),
                    locked: true,
                },
                CalendarEvent {
                    event_id: "evt-weekly-1-1".to_string(),
                    title: "Weekly 1:1".to_string(),
                    start_at: at_minute(11 * 60),
                    end_at: at_minute(11 * 60 + 30),
                    locked: false,
                },
                CalendarEvent {
                    event_id: "evt-planning-block".to_string(),
                    title: "Planning block".to_string(),
                    start_at: at_minute(13 * 60),
                    end_at: at_minute(15 * 60),
                    locked: true,
                },
                CalendarEvent {
                    event_id: "evt-hiring-panel".to_string(),
                    title: "Hiring panel".to_string(),
                    start_at: at_minute(15 * 60),
                    end_at: at_minute(17 * 60),
                    locked: true,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Tool)]
enum CalendarTools {
    ListEvents,
    MoveEvent {
        event_id: String,
        start_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CalendarToolResult {
    Events {
        events: Vec<CalendarEvent>,
        longest_free_block_minutes: u32,
    },
    Moved {
        events: Vec<CalendarEvent>,
        moved_events: usize,
        longest_free_block_minutes: u32,
        locked_events_preserved: bool,
        moved_title: String,
        new_start_at: DateTime<Utc>,
    },
    Impossible {
        reason: String,
        conflicting_event: Option<String>,
        events: Vec<CalendarEvent>,
        longest_free_block_minutes: u32,
    },
}

#[derive(Debug)]
struct InMemoryCalendar {
    working_day_start: DateTime<Utc>,
    working_day_end: DateTime<Utc>,
    original_events: Vec<CalendarEvent>,
    current_events: Vec<CalendarEvent>,
}

impl InMemoryCalendar {
    fn new(fixture: CalendarFixture) -> Self {
        Self {
            working_day_start: fixture.working_day_start,
            working_day_end: fixture.working_day_end,
            original_events: fixture.events.clone(),
            current_events: fixture.events,
        }
    }

    fn list_events(&self) -> CalendarToolResult {
        CalendarToolResult::Events {
            events: self.current_events.clone(),
            longest_free_block_minutes: self.longest_free_block_minutes(),
        }
    }

    fn move_event(&mut self, event_id: &str, start_at: DateTime<Utc>) -> CalendarToolResult {
        let Some(index) = self
            .current_events
            .iter()
            .position(|event| event.event_id == event_id)
        else {
            return CalendarToolResult::Impossible {
                reason: format!("event not found: {event_id}"),
                conflicting_event: None,
                events: self.current_events.clone(),
                longest_free_block_minutes: self.longest_free_block_minutes(),
            };
        };
        if self.current_events[index].locked {
            return CalendarToolResult::Impossible {
                reason: format!("event is locked: {event_id}"),
                conflicting_event: None,
                events: self.current_events.clone(),
                longest_free_block_minutes: self.longest_free_block_minutes(),
            };
        }

        let duration = self.current_events[index]
            .end_at
            .signed_duration_since(self.current_events[index].start_at);
        let end_at = start_at + duration;
        let title = self.current_events[index].title.clone();
        if start_at < self.working_day_start || end_at > self.working_day_end {
            return CalendarToolResult::Impossible {
                reason: format!("event would move outside working hours: {title}"),
                conflicting_event: None,
                events: self.current_events.clone(),
                longest_free_block_minutes: self.longest_free_block_minutes(),
            };
        }

        let conflicting_event = self
            .current_events
            .iter()
            .enumerate()
            .find(|(other_index, event)| {
                *other_index != index && start_at < event.end_at && end_at > event.start_at
            })
            .map(|(_, event)| event.title.clone());
        if let Some(conflicting_event) = conflicting_event {
            return CalendarToolResult::Impossible {
                reason: format!(
                    "event would overlap another event: {title} overlaps {conflicting_event}"
                ),
                conflicting_event: Some(conflicting_event),
                events: self.current_events.clone(),
                longest_free_block_minutes: self.longest_free_block_minutes(),
            };
        }

        self.current_events[index].start_at = start_at;
        self.current_events[index].end_at = end_at;
        self.current_events.sort_by_key(|event| event.start_at);

        let moved_events = count_moved_events(&self.original_events, &self.current_events);
        let locked_events_preserved =
            locked_events_match(&self.original_events, &self.current_events);

        CalendarToolResult::Moved {
            events: self.current_events.clone(),
            moved_events,
            longest_free_block_minutes: self.longest_free_block_minutes(),
            locked_events_preserved,
            moved_title: title.to_string(),
            new_start_at: start_at,
        }
    }

    fn snapshot(&self) -> CalendarSnapshot {
        CalendarSnapshot {
            working_day_start: self.working_day_start,
            working_day_end: self.working_day_end,
            original_events: self.original_events.clone(),
            current_events: self.current_events.clone(),
        }
    }

    fn longest_free_block_minutes(&self) -> u32 {
        longest_free_block_minutes(
            &self.current_events,
            self.working_day_start,
            self.working_day_end,
        )
    }
}

#[derive(Clone, Debug, Serialize)]
struct CalendarSnapshot {
    working_day_start: DateTime<Utc>,
    working_day_end: DateTime<Utc>,
    original_events: Vec<CalendarEvent>,
    current_events: Vec<CalendarEvent>,
}
