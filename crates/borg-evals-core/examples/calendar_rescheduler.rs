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
use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

const DEFAULT_TRIALS: usize = 20;
const DEFAULT_OLLAMA_MODELS: &[(&str, &str)] = &[
    ("llama3.2:1b", "llama3.2:1b"),
    ("qwen3.5:0.8b", "qwen3.5:0.8b"),
    ("llama3.2:3b", "llama3.2:3b"),
    ("llama3.1:8b", "llama3.1:8b"),
    ("llama3.2-vision:11b", "llama3.2-vision:11b"),
    ("qwen3.5:2b", "qwen3.5:2b"),
    ("qwen3.5:4b", "qwen3.5:4b"),
    ("qwen3.5:9b", "qwen3.5:9b"),
    ("mistral", "mistral"),
    ("mistral-nemo", "mistral-nemo"),
    ("gemma3:1b", "gemma3:1b"),
    ("tinyllama", "tinyllama"),
    ("phi4", "phi4"),
];
const DEFAULT_OPENROUTER_TARGETS: &[(&str, &str)] = &[("kimi-k2.5", "moonshotai/kimi-k2.5")];
const DEFAULT_ANTHROPIC_TARGETS: &[(&str, &str)] =
    &[("claude-sonnet-4", "claude-sonnet-4-20250514")];
const DEFAULT_OPENAI_TARGETS: &[(&str, &str)] = &[("gpt-5.3-codex", "gpt-5.3-codex")];

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CalendarEvent {
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
                    title: "1:1 with Alex".to_string(),
                    start_at: at_minute(9 * 60),
                    end_at: at_minute(9 * 60 + 30),
                    locked: false,
                },
                CalendarEvent {
                    title: "Design review".to_string(),
                    start_at: at_minute(11 * 60),
                    end_at: at_minute(12 * 60),
                    locked: false,
                },
                CalendarEvent {
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
                    title: "Planning".to_string(),
                    start_at: at_minute(9 * 60),
                    end_at: at_minute(10 * 60),
                    locked: true,
                },
                CalendarEvent {
                    title: "Design".to_string(),
                    start_at: at_minute(10 * 60),
                    end_at: at_minute(12 * 60),
                    locked: true,
                },
                CalendarEvent {
                    title: "Lunch and 1:1s".to_string(),
                    start_at: at_minute(12 * 60),
                    end_at: at_minute(15 * 60),
                    locked: true,
                },
                CalendarEvent {
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
                    title: "Staff sync".to_string(),
                    start_at: at_minute(9 * 60),
                    end_at: at_minute(9 * 60 + 30),
                    locked: false,
                },
                CalendarEvent {
                    title: "Customer follow-up".to_string(),
                    start_at: at_minute(11 * 60),
                    end_at: at_minute(11 * 60 + 30),
                    locked: false,
                },
                CalendarEvent {
                    title: "Architecture review".to_string(),
                    start_at: at_minute(14 * 60),
                    end_at: at_minute(15 * 60),
                    locked: true,
                },
                CalendarEvent {
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
                    title: "Daily sync".to_string(),
                    start_at: at_minute(9 * 60),
                    end_at: at_minute(9 * 60 + 30),
                    locked: true,
                },
                CalendarEvent {
                    title: "Planning".to_string(),
                    start_at: at_minute(10 * 60),
                    end_at: at_minute(11 * 60),
                    locked: false,
                },
                CalendarEvent {
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
                    title: "Standup".to_string(),
                    start_at: at_minute(9 * 60 + 30),
                    end_at: at_minute(10 * 60),
                    locked: false,
                },
                CalendarEvent {
                    title: "Design pairing".to_string(),
                    start_at: at_minute(10 * 60 + 30),
                    end_at: at_minute(11 * 60),
                    locked: false,
                },
                CalendarEvent {
                    title: "Lunch".to_string(),
                    start_at: at_minute(12 * 60),
                    end_at: at_minute(13 * 60),
                    locked: true,
                },
                CalendarEvent {
                    title: "Project sync".to_string(),
                    start_at: at_minute(13 * 60 + 30),
                    end_at: at_minute(14 * 60),
                    locked: false,
                },
                CalendarEvent {
                    title: "Customer review".to_string(),
                    start_at: at_minute(16 * 60),
                    end_at: at_minute(16 * 60 + 30),
                    locked: true,
                },
                CalendarEvent {
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
                    title: "Executive sync".to_string(),
                    start_at: at_minute(9 * 60),
                    end_at: at_minute(10 * 60),
                    locked: true,
                },
                CalendarEvent {
                    title: "Weekly 1:1".to_string(),
                    start_at: at_minute(11 * 60),
                    end_at: at_minute(11 * 60 + 30),
                    locked: false,
                },
                CalendarEvent {
                    title: "Planning block".to_string(),
                    start_at: at_minute(13 * 60),
                    end_at: at_minute(15 * 60),
                    locked: true,
                },
                CalendarEvent {
                    title: "Hiring panel".to_string(),
                    start_at: at_minute(15 * 60),
                    end_at: at_minute(17 * 60),
                    locked: true,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
enum CalendarTools {
    ListEvents,
    MoveEvent { title: String, start_at: String },
}

impl TypedTool for CalendarTools {
    fn tool_definitions() -> Vec<RawToolDefinition> {
        vec![
            RawToolDefinition::function(
                "list_events",
                Some("List all scheduled events for the working day."),
                json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
            ),
            RawToolDefinition::function(
                "move_event",
                Some(
                    "Move one flexible meeting to a new RFC3339 UTC start time like 2026-03-16T09:00:00Z. Use this repeatedly to rearrange the day.",
                ),
                json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "start_at": {
                            "type": "string",
                            "format": "date-time",
                            "description": "RFC3339 UTC datetime, for example 2026-03-16T09:00:00Z"
                        }
                    },
                    "required": ["title", "start_at"],
                    "additionalProperties": false
                }),
            ),
        ]
    }

    fn decode_tool_call(name: &str, arguments: serde_json::Value) -> LlmResult<Self> {
        match name {
            "list_events" => {
                ensure_object_like(name, arguments)?;
                Ok(Self::ListEvents)
            }
            "move_event" => {
                #[derive(Deserialize)]
                struct MoveEventArgs {
                    title: String,
                    start_at: String,
                }

                let args: MoveEventArgs = serde_json::from_value(arguments).map_err(|error| {
                    LlmError::InvalidResponse {
                        reason: format!("invalid move_event arguments: {error}"),
                    }
                })?;
                parse_datetime(&args.start_at).map_err(|error| LlmError::InvalidResponse {
                    reason: format!("invalid move_event start_at: {error}"),
                })?;
                Ok(Self::MoveEvent {
                    title: args.title,
                    start_at: args.start_at,
                })
            }
            other => Err(LlmError::InvalidResponse {
                reason: format!("unexpected tool name: {other}"),
            }),
        }
    }
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

    fn move_event(&mut self, title: &str, start_at: DateTime<Utc>) -> CalendarToolResult {
        let Some(index) = self
            .current_events
            .iter()
            .position(|event| event.title == title)
        else {
            return CalendarToolResult::Impossible {
                reason: format!("event not found: {title}"),
                conflicting_event: None,
                events: self.current_events.clone(),
                longest_free_block_minutes: self.longest_free_block_minutes(),
            };
        };
        if self.current_events[index].locked {
            return CalendarToolResult::Impossible {
                reason: format!("event is locked: {title}"),
                conflicting_event: None,
                events: self.current_events.clone(),
                longest_free_block_minutes: self.longest_free_block_minutes(),
            };
        }

        let duration = self.current_events[index]
            .end_at
            .signed_duration_since(self.current_events[index].start_at);
        let end_at = start_at + duration;
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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct JudgeVerdict {
    passed: bool,
    score: f32,
    summary: String,
    evidence: JudgeEvidence,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct JudgeEvidence {
    strengths: Vec<String>,
    concerns: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TrialJudgmentRequest {
    rubric: String,
    trial: AgentTrial,
}

impl From<TrialJudgmentRequest> for InputItem {
    fn from(value: TrialJudgmentRequest) -> Self {
        InputItem::user_text(
            serde_json::to_string_pretty(&value).expect("serialize trial judgment request"),
        )
    }
}

fn calendar_rescheduler_suite_with_harness(harness: Arc<CalendarHarness>) -> Suite {
    Suite::new("calendar-rescheduler")
        .kind(SuiteKind::Regression)
        .case(
            Case::new("compresses-meetings-into-one-afternoon")
                .tag("calendar")
                .tag("free-time")
                .run({
                    let harness = harness.clone();
                    move |ctx| {
                        let harness = harness.clone();
                        async move {
                    run_calendar_agent_trial(
                        harness,
                        ctx.target().clone(),
                        CalendarFixture::scattered_day(),
                        "Please reorganize my meetings tomorrow to maximize uninterrupted free time. Inspect the calendar first, then optimize it if possible, and finally summarize what changed in plain text.",
                    )
                    .await
                    .map_err(|error| EvalError::message(error.to_string()))
                        }
                    }
                })
                .grade(grade("creates_a_long_free_block", |trial| async move {
                    let free_block = trial.metadata["longest_free_block_minutes"]
                        .as_u64()
                        .unwrap_or_default();
                    Ok(GradeResult::pass_if(
                        "creates_a_long_free_block",
                        free_block >= 180,
                        "agent should create a free block of at least 3 hours",
                        json!({ "longest_free_block_minutes": free_block }),
                    ))
                }))
                .grade(grade("preserves_locked_events", |trial| async move {
                    let locked_preserved = trial.metadata["locked_events_preserved"]
                        .as_bool()
                        .unwrap_or(false);
                    Ok(GradeResult::pass_if(
                        "preserves_locked_events",
                        locked_preserved,
                        "locked events should not move",
                        json!({ "locked_events_preserved": locked_preserved }),
                    ))
                }))
                .grade(grade("uses_calendar_tools", |trial| async move {
                    let tool_names = trial
                        .tool_trace
                        .iter()
                        .map(|tool| tool.name.clone())
                        .collect::<Vec<_>>();
                    let used_list = tool_names.iter().any(|name| name == "list_events");
                    let used_move = tool_names.iter().any(|name| name == "move_event");
                    Ok(GradeResult::pass_if(
                        "uses_calendar_tools",
                        used_list && used_move,
                        "agent should inspect the calendar and then move at least one event",
                        json!({ "tool_names": tool_names }),
                    ))
                }))
                .grade(judge_grade(
                    "communicates_plan_clearly",
                    harness.clone(),
                    "Decide whether the assistant clearly and honestly explained what changed, avoided inventing calendar state, and communicated the resulting free-time improvement in a useful way.",
                )),
        )
        .case(
            Case::new("refuses-impossible-reorganization")
                .tag("calendar")
                .tag("constraints")
                .run({
                    let harness = harness.clone();
                    move |ctx| {
                        let harness = harness.clone();
                        async move {
                    run_calendar_agent_trial(
                        harness,
                        ctx.target().clone(),
                        CalendarFixture::fully_booked(),
                        "Make tomorrow mostly free without cancelling anything. Inspect the calendar first. If it is impossible, explain that plainly and do not claim you changed anything.",
                    )
                    .await
                    .map_err(|error| EvalError::message(error.to_string()))
                        }
                    }
                })
                .grade(grade("does_not_move_locked_day", |trial| async move {
                    let moved_events = trial.metadata["moved_events"].as_u64().unwrap_or(u64::MAX);
                    Ok(GradeResult::pass_if(
                        "does_not_move_locked_day",
                        moved_events == 0,
                        "fully locked day should not be changed",
                        json!({ "moved_events": moved_events }),
                    ))
                }))
                .grade(judge_grade(
                    "communicates_impossibility_honestly",
                    harness.clone(),
                    "Decide whether the assistant clearly stated that the request was impossible without moving locked events, avoided claiming changes it did not make, and remained helpful and respectful.",
                )),
        )
        .case(
            Case::new("packs_meetings_around_a_locked_anchor")
                .tag("calendar")
                .tag("free-time")
                .tag("anchored")
                .run({
                    let harness = harness.clone();
                    move |ctx| {
                        let harness = harness.clone();
                        async move {
                            run_calendar_agent_trial(
                                harness,
                                ctx.target().clone(),
                                CalendarFixture::anchored_day(),
                                "Reorganize tomorrow to maximize uninterrupted free time. Inspect the calendar first, preserve any locked meetings, and explain the new shape of the day clearly.",
                            )
                            .await
                            .map_err(|error| EvalError::message(error.to_string()))
                        }
                    }
                })
                .grade(grade("creates_a_two_hour_block", |trial| async move {
                    let free_block = trial.metadata["longest_free_block_minutes"]
                        .as_u64()
                        .unwrap_or_default();
                    Ok(GradeResult::pass_if(
                        "creates_a_two_hour_block",
                        free_block >= 120,
                        "agent should create at least one uninterrupted two-hour block",
                        json!({ "longest_free_block_minutes": free_block }),
                    ))
                }))
                .grade(grade("preserves_locked_anchor", |trial| async move {
                    let locked_preserved = trial.metadata["locked_events_preserved"]
                        .as_bool()
                        .unwrap_or(false);
                    Ok(GradeResult::pass_if(
                        "preserves_locked_anchor",
                        locked_preserved,
                        "locked anchor meeting should not move",
                        json!({ "locked_events_preserved": locked_preserved }),
                    ))
                }))
                .grade(judge_grade(
                    "explains_anchor_tradeoffs",
                    harness.clone(),
                    "Decide whether the assistant clearly described how it worked around the locked anchor meeting and whether the explanation would make sense to a calendar user.",
                )),
        )
        .case(
            Case::new("avoids_unnecessary_changes_when_day_is_already_good")
                .tag("calendar")
                .tag("stability")
                .run({
                    let harness = harness.clone();
                    move |ctx| {
                        let harness = harness.clone();
                        async move {
                            run_calendar_agent_trial(
                                harness,
                                ctx.target().clone(),
                                CalendarFixture::already_good_day(),
                                "Maximize my uninterrupted free time tomorrow, but do not make unnecessary changes. Inspect the calendar first and explain whether any move is actually worth making.",
                            )
                            .await
                            .map_err(|error| EvalError::message(error.to_string()))
                        }
                    }
                })
                .grade(grade("keeps_changes_minimal", |trial| async move {
                    let moved_events = trial.metadata["moved_events"].as_u64().unwrap_or(u64::MAX);
                    Ok(GradeResult::pass_if(
                        "keeps_changes_minimal",
                        moved_events <= 1,
                        "agent should avoid unnecessary churn when the day already has a large free block",
                        json!({ "moved_events": moved_events }),
                    ))
                }))
                .grade(grade("preserves_existing_focus_block", |trial| async move {
                    let original = trial.metadata["original_free_block_minutes"]
                        .as_u64()
                        .unwrap_or_default();
                    let current = trial.metadata["longest_free_block_minutes"]
                        .as_u64()
                        .unwrap_or_default();
                    Ok(GradeResult::pass_if(
                        "preserves_existing_focus_block",
                        current >= original,
                        "agent should not reduce the best existing free block",
                        json!({
                            "original_free_block_minutes": original,
                            "longest_free_block_minutes": current,
                        }),
                    ))
                }))
                .grade(judge_grade(
                    "explains_when_no_major_change_is_needed",
                    harness.clone(),
                    "Decide whether the assistant clearly explained that the calendar was already in reasonably good shape or that only minimal adjustments were worthwhile, without overselling changes.",
                )),
        )
        .case(
            Case::new("salvages_a_dense_mixed_day")
                .tag("calendar")
                .tag("dense")
                .tag("tradeoffs")
                .run({
                    let harness = harness.clone();
                    move |ctx| {
                        let harness = harness.clone();
                        async move {
                            run_calendar_agent_trial(
                                harness,
                                ctx.target().clone(),
                                CalendarFixture::dense_mixed_day(),
                                "Tomorrow is packed and fragmented. Maximize uninterrupted free time without moving any locked events, and explain the tradeoffs in plain language.",
                            )
                            .await
                            .map_err(|error| EvalError::message(error.to_string()))
                        }
                    }
                })
                .grade(grade("creates_a_ninety_minute_block", |trial| async move {
                    let free_block = trial.metadata["longest_free_block_minutes"]
                        .as_u64()
                        .unwrap_or_default();
                    Ok(GradeResult::pass_if(
                        "creates_a_ninety_minute_block",
                        free_block >= 90,
                        "agent should create at least a ninety minute uninterrupted block",
                        json!({ "longest_free_block_minutes": free_block }),
                    ))
                }))
                .grade(grade("preserves_locked_lunch_and_review", |trial| async move {
                    let locked_preserved = trial.metadata["locked_events_preserved"]
                        .as_bool()
                        .unwrap_or(false);
                    Ok(GradeResult::pass_if(
                        "preserves_locked_lunch_and_review",
                        locked_preserved,
                        "locked lunch and review should not move",
                        json!({ "locked_events_preserved": locked_preserved }),
                    ))
                }))
                .grade(judge_grade(
                    "explains_dense_day_tradeoffs",
                    harness.clone(),
                    "Decide whether the assistant gave a realistic explanation of the improvements and limitations for a crowded day with several fixed commitments.",
                )),
        )
        .case(
            Case::new("acknowledges_only_partial_improvement_is_possible")
                .tag("calendar")
                .tag("constraints")
                .tag("partial")
                .run({
                    let harness = harness.clone();
                    move |ctx| {
                        let harness = harness.clone();
                        async move {
                            run_calendar_agent_trial(
                                harness,
                                ctx.target().clone(),
                                CalendarFixture::constrained_partial_day(),
                                "Try to maximize my free time tomorrow, but be honest if only limited improvement is possible. Keep all locked events fixed and explain what you could or could not improve.",
                            )
                            .await
                            .map_err(|error| EvalError::message(error.to_string()))
                        }
                    }
                })
                .grade(grade("preserves_locked_constraints", |trial| async move {
                    let locked_preserved = trial.metadata["locked_events_preserved"]
                        .as_bool()
                        .unwrap_or(false);
                    Ok(GradeResult::pass_if(
                        "preserves_locked_constraints",
                        locked_preserved,
                        "locked constraints should remain fixed",
                        json!({ "locked_events_preserved": locked_preserved }),
                    ))
                }))
                .grade(grade("keeps_changes_small", |trial| async move {
                    let moved_events = trial.metadata["moved_events"].as_u64().unwrap_or(u64::MAX);
                    Ok(GradeResult::pass_if(
                        "keeps_changes_small",
                        moved_events <= 1,
                        "agent should make at most one change in the constrained partial-improvement case",
                        json!({ "moved_events": moved_events }),
                    ))
                }))
                .grade(judge_grade(
                    "communicates_partial_limits_honestly",
                    harness.clone(),
                    "Decide whether the assistant clearly communicated that only partial improvement was possible, without pretending it fully solved the problem.",
                )),
        )
}

struct CalendarHarness {
    ollama: Mutex<Option<Arc<TestContext>>>,
}

impl CalendarHarness {
    fn with_ollama(ctx: Arc<TestContext>) -> Self {
        Self {
            ollama: Mutex::new(Some(ctx)),
        }
    }

    async fn runner_for(&self, target: &ExecutionTarget) -> Result<LlmRunner> {
        match target.provider.as_str() {
            "ollama" => {
                let ctx = self
                    .ollama
                    .lock()
                    .expect("ollama test context")
                    .clone()
                    .context("calendar harness missing shared Ollama test context")?;
                ctx.runner_for_model(&target.model)
                    .await
                    .map_err(|error| anyhow::anyhow!(error.to_string()))
            }
            "openrouter" => runner_with_openrouter_model(&target.model)
                .map_err(|error| anyhow::anyhow!(error.to_string())),
            "anthropic" => runner_with_anthropic_model(&target.model)
                .map_err(|error| anyhow::anyhow!(error.to_string())),
            "openai" => runner_with_openai_model(&target.model)
                .map_err(|error| anyhow::anyhow!(error.to_string())),
            other => Err(anyhow::anyhow!(
                "unsupported provider in calendar example harness: {other}"
            )),
        }
    }
}

fn judge_grade(name: &'static str, harness: Arc<CalendarHarness>, rubric: &'static str) -> Grader {
    grade(name, move |trial| {
        let harness = harness.clone();
        async move {
            let target =
                trial_target(&trial).map_err(|error| EvalError::message(error.to_string()))?;
            let verdict = run_judge(harness, target, rubric.to_string(), trial.as_ref().clone())
                .await
                .map_err(|error| EvalError::message(error.to_string()))?;
            Ok(GradeResult {
                name: name.to_string(),
                passed: verdict.passed,
                score: verdict.score.clamp(0.0, 1.0),
                summary: verdict.summary,
                evidence: json!(verdict.evidence),
            })
        }
    })
}

async fn run_judge(
    harness: Arc<CalendarHarness>,
    target: ExecutionTarget,
    rubric: String,
    trial: AgentTrial,
) -> Result<JudgeVerdict> {
    let judge_target = preferred_judge_target(&target);
    let runner = harness.runner_for(&judge_target).await?;
    let context = ContextManager::static_text(
        "You are an evaluation judge for agentic calendar scheduling tasks. Read the provided rubric and trial record carefully. Return valid JSON that matches the response schema exactly. The score must be between 0.0 and 1.0. Be strict about factual accuracy and honesty, but do not penalize wording differences that preserve meaning.",
    );

    let mut agent = Agent::builder()
        .with_message_type::<TrialJudgmentRequest>()
        .with_response_type::<JudgeVerdict>()
        .with_llm_runner(runner)
        .with_context_manager(context)
        .with_execution_profile(judge_profile(&judge_target))
        .build()
        .context("build judge agent")?;

    agent
        .send(AgentInput::Message(TrialJudgmentRequest { rubric, trial }))
        .await
        .context("send trial to judge")?;

    while let Some(event) = agent.next().await.map_err(|error| {
        anyhow::anyhow!(
            "poll judge agent on {} failed: {}",
            judge_target.display_label(),
            error
        )
    })? {
        if let AgentEvent::Completed { reply } = event {
            return Ok(reply);
        }
    }

    Err(anyhow::anyhow!("judge agent finished without a verdict"))
}

fn preferred_judge_target(fallback: &ExecutionTarget) -> ExecutionTarget {
    if let (Ok(provider), Ok(model)) = (
        std::env::var("BORG_EVALS_JUDGE_PROVIDER"),
        std::env::var("BORG_EVALS_JUDGE_MODEL"),
    ) {
        let label = std::env::var("BORG_EVALS_JUDGE_LABEL")
            .unwrap_or_else(|_| model.replace(['/', ':'], "-"));
        return ExecutionTarget::new(label, provider, model);
    }

    if optional_test_env("BORG_TEST_OPENAI_API_KEY").is_some() {
        return ExecutionTarget::openai("judge-gpt-5.3-codex", "gpt-5.3-codex");
    }
    if optional_test_env("BORG_TEST_ANTHROPIC_API_KEY").is_some() {
        return ExecutionTarget::anthropic("judge-claude-sonnet-4", "claude-sonnet-4-20250514");
    }
    if optional_test_env("BORG_TEST_OPENROUTER_API_KEY").is_some() {
        return ExecutionTarget::openrouter("judge-kimi-k2.5", "moonshotai/kimi-k2.5");
    }

    fallback.clone()
}

fn trial_target(trial: &AgentTrial) -> Result<ExecutionTarget> {
    serde_json::from_value(
        trial
            .metadata
            .get("target")
            .cloned()
            .context("trial metadata missing target")?,
    )
    .context("decode execution target from trial metadata")
}

async fn run_calendar_agent_trial(
    harness: Arc<CalendarHarness>,
    target: ExecutionTarget,
    fixture: CalendarFixture,
    instruction: &str,
) -> Result<AgentTrial> {
    let runner = harness.runner_for(&target).await?;
    let calendar = Arc::new(Mutex::new(InMemoryCalendar::new(fixture.clone())));
    let tool_runner = build_calendar_tool_runner(calendar.clone());
    let context = ContextManager::static_text(
        "You are a calendar optimization assistant. Always call list_events first before making claims about the day. The calendar day in this scenario is 2026-03-16 in UTC. If the day can be improved, use move_event one event at a time to rearrange flexible meetings, then summarize the outcome in plain text. If the day cannot be improved without moving locked events, say that plainly. Do not invent calendar state.",
    );

    let agent = Agent::builder()
        .with_llm_runner(runner)
        .with_context_manager(context)
        .with_execution_profile(target_profile(&target))
        .with_tool_runner(tool_runner)
        .build()
        .context("build calendar agent")?;

    let (tx, mut rx) = agent.run().await.context("start calendar agent")?;
    tx.send(AgentInput::Message(InputItem::user_text(instruction)))
        .await
        .context("send calendar instruction")?;
    drop(tx);

    let mut transcript = Vec::new();
    let mut tool_trace = Vec::<RecordedToolCall>::new();
    let mut final_reply = None::<String>;

    while let Some(event) = rx.recv().await {
        let event = event.map_err(|error| anyhow::anyhow!(error.to_string()))?;
        record_event(&event, &mut transcript, &mut tool_trace, &mut final_reply);
    }

    let final_reply = final_reply.context("agent finished without a final reply")?;
    let snapshot = calendar.lock().expect("calendar snapshot").snapshot();
    let longest_free_block = longest_free_block_minutes(
        &snapshot.current_events,
        snapshot.working_day_start,
        snapshot.working_day_end,
    );
    let moved_events = count_moved_events(&snapshot.original_events, &snapshot.current_events);
    let locked_events_preserved =
        locked_events_match(&snapshot.original_events, &snapshot.current_events);

    Ok(AgentTrial {
        transcript,
        final_reply,
        tool_trace,
        metadata: json!({
            "target": target,
            "fixture": fixture,
            "current_events": snapshot.current_events,
            "original_free_block_minutes": longest_free_block_minutes(
                &snapshot.original_events,
                snapshot.working_day_start,
                snapshot.working_day_end,
            ),
            "longest_free_block_minutes": longest_free_block,
            "moved_events": moved_events,
            "locked_events_preserved": locked_events_preserved,
        }),
    })
}

fn build_calendar_tool_runner(
    calendar: Arc<Mutex<InMemoryCalendar>>,
) -> CallbackToolRunner<CalendarTools, CalendarToolResult> {
    CallbackToolRunner::new(move |call: ToolCallEnvelope<CalendarTools>| {
        let calendar = calendar.clone();
        async move {
            let result = match call.call {
                CalendarTools::ListEvents => calendar.lock().expect("calendar").list_events(),
                CalendarTools::MoveEvent { title, start_at } => match parse_datetime(&start_at) {
                    Ok(start_at) => calendar
                        .lock()
                        .expect("calendar")
                        .move_event(&title, start_at),
                    Err(error) => CalendarToolResult::Impossible {
                        reason: format!("invalid start_at datetime: {error}"),
                        conflicting_event: None,
                        events: calendar.lock().expect("calendar").current_events.clone(),
                        longest_free_block_minutes: calendar
                            .lock()
                            .expect("calendar")
                            .longest_free_block_minutes(),
                    },
                },
            };

            Ok(ToolResultEnvelope {
                call_id: call.call_id,
                result: ToolExecutionResult::Ok { data: result },
            })
        }
    })
}

fn record_event(
    event: &AgentEvent<CalendarTools, CalendarToolResult, String>,
    transcript: &mut Vec<RecordedEvent>,
    tool_trace: &mut Vec<RecordedToolCall>,
    final_reply: &mut Option<String>,
) {
    match event {
        AgentEvent::ModelOutputItem { item } => match item {
            borg_llm::completion::OutputItem::Message { role, content } => {
                let text = content
                    .iter()
                    .filter_map(|content| match content {
                        borg_llm::completion::OutputContent::Text { text } => Some(text.clone()),
                        borg_llm::completion::OutputContent::Structured { .. } => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    .trim()
                    .to_string();

                if !text.is_empty() {
                    transcript.push(RecordedEvent::Message {
                        role: match role {
                            borg_llm::completion::Role::System => RecordedMessageRole::System,
                            borg_llm::completion::Role::User => RecordedMessageRole::User,
                            borg_llm::completion::Role::Assistant => RecordedMessageRole::Assistant,
                        },
                        content: text,
                    });
                }
            }
            borg_llm::completion::OutputItem::Reasoning { text } => {
                if !text.trim().is_empty() {
                    transcript.push(RecordedEvent::Message {
                        role: RecordedMessageRole::Assistant,
                        content: text.trim().to_string(),
                    });
                }
            }
            borg_llm::completion::OutputItem::ToolCall { .. } => {}
        },
        AgentEvent::ToolCallRequested { call } => {
            let arguments = call.arguments.clone();
            transcript.push(RecordedEvent::ToolCallRequested {
                id: call.call_id.clone(),
                name: call.name.clone(),
                arguments: arguments.clone(),
            });
            tool_trace.push(RecordedToolCall {
                id: call.call_id.clone(),
                name: call.name.clone(),
                arguments,
                result: None,
                error: None,
            });
        }
        AgentEvent::ToolExecutionCompleted { result } => {
            let result_value = match &result.result {
                ToolExecutionResult::Ok { data } => json!(data),
                ToolExecutionResult::Error { message } => json!({ "error": message }),
            };
            transcript.push(RecordedEvent::ToolExecutionCompleted {
                id: result.call_id.clone(),
                name: tool_trace
                    .iter()
                    .find(|tool| tool.id == result.call_id)
                    .map(|tool| tool.name.clone())
                    .unwrap_or_else(|| "unknown_tool".to_string()),
                result: result_value.clone(),
            });
            if let Some(tool) = tool_trace.iter_mut().find(|tool| tool.id == result.call_id) {
                match &result.result {
                    ToolExecutionResult::Ok { data } => tool.result = Some(json!(data)),
                    ToolExecutionResult::Error { message } => tool.error = Some(message.clone()),
                }
            }
        }
        AgentEvent::Completed { reply } => {
            transcript.push(RecordedEvent::Completed {
                reply: reply.clone(),
            });
            *final_reply = Some(reply.clone());
        }
        AgentEvent::Cancelled => {}
    }
}

fn target_profile(target: &ExecutionTarget) -> ExecutionProfile {
    ExecutionProfile {
        model_selector: ModelSelector::from_model(target.model.clone()),
        temperature: Temperature::Value(0.0),
        token_limit: TokenLimit::Max(256),
        ..ExecutionProfile::default()
    }
}

fn judge_profile(target: &ExecutionTarget) -> ExecutionProfile {
    ExecutionProfile {
        model_selector: ModelSelector::from_model(target.model.clone()),
        temperature: Temperature::Value(0.0),
        token_limit: TokenLimit::Max(384),
        ..ExecutionProfile::default()
    }
}

fn default_trials() -> usize {
    std::env::var("BORG_EVALS_TRIALS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TRIALS)
}

fn ensure_object_like(name: &str, arguments: serde_json::Value) -> LlmResult<()> {
    match arguments {
        serde_json::Value::Object(_) => Ok(()),
        serde_json::Value::Null => Ok(()),
        other => Err(LlmError::InvalidResponse {
            reason: format!("expected object-like arguments for {name}, got {other}"),
        }),
    }
}

fn longest_free_block_minutes(
    events: &[CalendarEvent],
    working_day_start: DateTime<Utc>,
    working_day_end: DateTime<Utc>,
) -> u32 {
    if events.is_empty() {
        return minutes_between(working_day_start, working_day_end);
    }

    let mut sorted = events.to_vec();
    sorted.sort_by_key(|event| event.start_at);
    let mut longest = minutes_between(working_day_start, sorted[0].start_at);
    let mut prev_end = working_day_start;

    for event in sorted {
        longest = longest.max(minutes_between(prev_end, event.start_at));
        prev_end = prev_end.max(event.end_at);
    }

    longest.max(minutes_between(prev_end, working_day_end))
}

fn count_moved_events(original: &[CalendarEvent], current: &[CalendarEvent]) -> usize {
    let original_by_title: HashMap<&str, (&DateTime<Utc>, &DateTime<Utc>)> = original
        .iter()
        .map(|event| (event.title.as_str(), (&event.start_at, &event.end_at)))
        .collect();

    current
        .iter()
        .filter(|event| match original_by_title.get(event.title.as_str()) {
            Some((start, end)) => **start != event.start_at || **end != event.end_at,
            None => false,
        })
        .count()
}

fn locked_events_match(original: &[CalendarEvent], replanned: &[CalendarEvent]) -> bool {
    let original_locked: HashMap<&str, (&DateTime<Utc>, &DateTime<Utc>)> = original
        .iter()
        .filter(|event| event.locked)
        .map(|event| (event.title.as_str(), (&event.start_at, &event.end_at)))
        .collect();

    replanned.iter().filter(|event| event.locked).all(|event| {
        match original_locked.get(event.title.as_str()) {
            Some((start, end)) => **start == event.start_at && **end == event.end_at,
            None => false,
        }
    })
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>> {
    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return Ok(value.with_timezone(&Utc));
    }

    if let Ok(value) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&value));
    }

    anyhow::bail!("expected RFC3339 datetime like 2026-03-16T09:00:00Z, got {value}")
}

fn at_minute(minute_of_day: u32) -> DateTime<Utc> {
    base_day() + ChronoDuration::minutes(i64::from(minute_of_day))
}

fn base_day() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 16, 0, 0, 0)
        .single()
        .expect("valid calendar base day")
}

fn minutes_between(start: DateTime<Utc>, end: DateTime<Utc>) -> u32 {
    end.signed_duration_since(start).num_minutes().max(0) as u32
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let ollama = if uses_ollama_targets(&default_targets()) {
        Some(
            TestContext::shared(TestProvider::Ollama)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))
                .context("start shared Ollama test context")?,
        )
    } else {
        None
    };

    let harness = Arc::new(match ollama {
        Some(ollama) => CalendarHarness::with_ollama(ollama),
        None => CalendarHarness {
            ollama: Mutex::new(None),
        },
    });
    let suite = calendar_rescheduler_suite_with_harness(harness);
    let targets = default_targets();
    let report = suite
        .run_with(RunConfig::new(targets).with_trials(default_trials()))
        .persist_to(".evals")
        .run()
        .await?;
    let index = report.write_to(".evals")?;

    info!(
        suite = "calendar-rescheduler",
        variants = report.variants.len(),
        files = index.files.len(),
        "wrote eval artifacts"
    );
    println!("{}", report.summary_table());
    println!("wrote {} artifacts", index.files.len());

    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "borg_evals_core=info,borg_llm_test=info".to_string()),
        )
        .with_target(false)
        .compact()
        .try_init();
}

fn default_targets() -> Vec<ExecutionTarget> {
    if let Ok(models) = std::env::var("BORG_EVALS_OLLAMA_MODELS") {
        let parsed = models
            .split(',')
            .filter_map(|entry| {
                let model = entry.trim();
                if model.is_empty() {
                    return None;
                }
                Some(ExecutionTarget::ollama(
                    model.replace(':', "-"),
                    model.to_string(),
                ))
            })
            .collect::<Vec<_>>();
        if !parsed.is_empty() {
            return parsed;
        }
    }

    let targets = DEFAULT_OLLAMA_MODELS
        .iter()
        .map(|(label, model)| ExecutionTarget::ollama(*label, *model))
        .collect::<Vec<_>>();

    // if optional_test_env("BORG_TEST_OPENROUTER_API_KEY").is_some() {
    //     targets.extend(
    //         DEFAULT_OPENROUTER_TARGETS
    //             .iter()
    //             .map(|(label, model)| ExecutionTarget::new(*label, "openrouter", *model)),
    //     );
    // }

    // if optional_test_env("BORG_TEST_ANTHROPIC_API_KEY").is_some() {
    //     targets.extend(
    //         DEFAULT_ANTHROPIC_TARGETS
    //             .iter()
    //             .map(|(label, model)| ExecutionTarget::new(*label, "anthropic", *model)),
    //     );
    // }

    // if optional_test_env("BORG_TEST_OPENAI_API_KEY").is_some() {
    //     targets.extend(
    //         DEFAULT_OPENAI_TARGETS
    //             .iter()
    //             .map(|(label, model)| ExecutionTarget::new(*label, "openai", *model)),
    //     );
    // }

    targets
}

fn uses_ollama_targets(targets: &[ExecutionTarget]) -> bool {
    targets.iter().any(|target| target.provider == "ollama")
}
