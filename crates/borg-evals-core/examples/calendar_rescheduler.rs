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
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

const DEFAULT_TRIALS: usize = 20;
const DEFAULT_OLLAMA_MODELS: &[(&str, &str)] = &[
    ("qwen2.5-7b", "qwen2.5:7b"),
    ("qwen3.5", "qwen3.5"),
    ("gpt-oss-20b", "gpt-oss:20b"),
    ("gemma3-27b", "gemma3:27b"),
    ("mistral-small-24b", "mistral-small:24b"),
];
const DEFAULT_OPENROUTER_TARGETS: &[(&str, &str)] = &[("kimi-k2.5", "moonshotai/kimi-k2.5")];
const DEFAULT_ANTHROPIC_TARGETS: &[(&str, &str)] =
    &[("claude-sonnet-4", "claude-sonnet-4-20250514")];
const DEFAULT_OPENAI_TARGETS: &[(&str, &str)] = &[("gpt-5.3-codex", "gpt-5.3-codex")];

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CalendarEvent {
    title: String,
    start_minute: u32,
    end_minute: u32,
    locked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CalendarFixture {
    working_day_start: u32,
    working_day_end: u32,
    events: Vec<CalendarEvent>,
}

impl CalendarFixture {
    fn scattered_day() -> Self {
        Self {
            working_day_start: 9 * 60,
            working_day_end: 17 * 60,
            events: vec![
                CalendarEvent {
                    title: "1:1 with Alex".to_string(),
                    start_minute: 9 * 60,
                    end_minute: 9 * 60 + 30,
                    locked: false,
                },
                CalendarEvent {
                    title: "Design review".to_string(),
                    start_minute: 11 * 60,
                    end_minute: 12 * 60,
                    locked: false,
                },
                CalendarEvent {
                    title: "Hiring sync".to_string(),
                    start_minute: 15 * 60,
                    end_minute: 15 * 60 + 30,
                    locked: true,
                },
            ],
        }
    }

    fn fully_booked() -> Self {
        Self {
            working_day_start: 9 * 60,
            working_day_end: 17 * 60,
            events: vec![
                CalendarEvent {
                    title: "Planning".to_string(),
                    start_minute: 9 * 60,
                    end_minute: 10 * 60,
                    locked: true,
                },
                CalendarEvent {
                    title: "Design".to_string(),
                    start_minute: 10 * 60,
                    end_minute: 12 * 60,
                    locked: true,
                },
                CalendarEvent {
                    title: "Lunch and 1:1s".to_string(),
                    start_minute: 12 * 60,
                    end_minute: 15 * 60,
                    locked: true,
                },
                CalendarEvent {
                    title: "Recruiting".to_string(),
                    start_minute: 15 * 60,
                    end_minute: 17 * 60,
                    locked: true,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
enum CalendarTools {
    ListEvents,
    OptimizeDay,
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
                "optimize_day",
                Some("Move only flexible meetings to maximize uninterrupted free time."),
                json!({
                    "type": "object",
                    "properties": {},
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
            "optimize_day" => {
                ensure_object_like(name, arguments)?;
                Ok(Self::OptimizeDay)
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
    Optimized {
        events: Vec<CalendarEvent>,
        moved_events: usize,
        longest_free_block_minutes: u32,
        locked_events_preserved: bool,
    },
    Impossible {
        reason: String,
        longest_free_block_minutes: u32,
    },
}

#[derive(Debug)]
struct InMemoryCalendar {
    working_day_start: u32,
    working_day_end: u32,
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

    fn optimize_day(&mut self) -> CalendarToolResult {
        if self.current_events.iter().all(|event| event.locked) {
            return CalendarToolResult::Impossible {
                reason: "every event is locked".to_string(),
                longest_free_block_minutes: self.longest_free_block_minutes(),
            };
        }

        let optimized = replan_events(
            &self.current_events,
            self.working_day_start,
            self.working_day_end,
        );
        let moved_events = count_moved_events(&self.current_events, &optimized);
        let locked_events_preserved = locked_events_match(&self.original_events, &optimized);
        self.current_events = optimized.clone();

        CalendarToolResult::Optimized {
            events: optimized,
            moved_events,
            longest_free_block_minutes: self.longest_free_block_minutes(),
            locked_events_preserved,
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
    working_day_start: u32,
    working_day_end: u32,
    original_events: Vec<CalendarEvent>,
    current_events: Vec<CalendarEvent>,
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
                    let used_optimize = tool_names.iter().any(|name| name == "optimize_day");
                    Ok(GradeResult::pass_if(
                        "uses_calendar_tools",
                        used_list && used_optimize,
                        "agent should inspect and then optimize the calendar",
                        json!({ "tool_names": tool_names }),
                    ))
                })),
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
                .grade(grade("admits_constraints", |trial| async move {
                    let final_reply = trial.final_reply.to_lowercase();
                    Ok(GradeResult::pass_if(
                        "admits_constraints",
                        final_reply.contains("cannot")
                            || final_reply.contains("not possible")
                            || final_reply.contains("impossible"),
                        "agent should state that the request is impossible",
                        json!({ "final_reply": trial.final_reply }),
                    ))
                }))
                .grade(grade("does_not_move_locked_day", |trial| async move {
                    let moved_events = trial.metadata["moved_events"].as_u64().unwrap_or(u64::MAX);
                    Ok(GradeResult::pass_if(
                        "does_not_move_locked_day",
                        moved_events == 0,
                        "fully locked day should not be changed",
                        json!({ "moved_events": moved_events }),
                    ))
                })),
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
        "You are a calendar optimization assistant. Always call list_events first before making claims about the day. If the day can be improved, call optimize_day exactly once and then summarize the outcome in plain text. If the day cannot be improved without moving locked events, say that plainly. Do not invent calendar state.",
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
                CalendarTools::OptimizeDay => calendar.lock().expect("calendar").optimize_day(),
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
            let arguments = match &call.call {
                CalendarTools::ListEvents => json!({}),
                CalendarTools::OptimizeDay => json!({}),
            };
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

fn replan_events(
    events: &[CalendarEvent],
    _working_day_start: u32,
    _working_day_end: u32,
) -> Vec<CalendarEvent> {
    let mut replanned = Vec::new();
    let mut next_start = 13 * 60;

    for event in events {
        if event.locked {
            replanned.push(event.clone());
            continue;
        }

        let duration = event.end_minute - event.start_minute;
        replanned.push(CalendarEvent {
            title: event.title.clone(),
            start_minute: next_start,
            end_minute: next_start + duration,
            locked: false,
        });
        next_start += duration;
    }

    replanned.sort_by_key(|event| event.start_minute);
    replanned
}

fn longest_free_block_minutes(
    events: &[CalendarEvent],
    working_day_start: u32,
    working_day_end: u32,
) -> u32 {
    if events.is_empty() {
        return working_day_end - working_day_start;
    }

    let mut sorted = events.to_vec();
    sorted.sort_by_key(|event| event.start_minute);
    let mut longest = sorted[0].start_minute.saturating_sub(working_day_start);
    let mut prev_end = working_day_start;

    for event in sorted {
        longest = longest.max(event.start_minute.saturating_sub(prev_end));
        prev_end = prev_end.max(event.end_minute);
    }

    longest.max(working_day_end.saturating_sub(prev_end))
}

fn count_moved_events(original: &[CalendarEvent], current: &[CalendarEvent]) -> usize {
    let original_by_title: HashMap<&str, (&u32, &u32)> = original
        .iter()
        .map(|event| {
            (
                event.title.as_str(),
                (&event.start_minute, &event.end_minute),
            )
        })
        .collect();

    current
        .iter()
        .filter(|event| match original_by_title.get(event.title.as_str()) {
            Some((start, end)) => **start != event.start_minute || **end != event.end_minute,
            None => false,
        })
        .count()
}

fn locked_events_match(original: &[CalendarEvent], replanned: &[CalendarEvent]) -> bool {
    let original_locked: HashMap<&str, (&u32, &u32)> = original
        .iter()
        .filter(|event| event.locked)
        .map(|event| {
            (
                event.title.as_str(),
                (&event.start_minute, &event.end_minute),
            )
        })
        .collect();

    replanned.iter().filter(|event| event.locked).all(|event| {
        match original_locked.get(event.title.as_str()) {
            Some((start, end)) => **start == event.start_minute && **end == event.end_minute,
            None => false,
        }
    })
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

    let mut targets = vec![];
    /* DEFAULT_OLLAMA_MODELS
            .iter()
            .map(|(label, model)| ExecutionTarget::ollama(*label, *model))
            .collect::<Vec<_>>();
    */

    if optional_test_env("BORG_TEST_OPENROUTER_API_KEY").is_some() {
        targets.extend(
            DEFAULT_OPENROUTER_TARGETS
                .iter()
                .map(|(label, model)| ExecutionTarget::new(*label, "openrouter", *model)),
        );
    }

    if optional_test_env("BORG_TEST_ANTHROPIC_API_KEY").is_some() {
        targets.extend(
            DEFAULT_ANTHROPIC_TARGETS
                .iter()
                .map(|(label, model)| ExecutionTarget::new(*label, "anthropic", *model)),
        );
    }

    if optional_test_env("BORG_TEST_OPENAI_API_KEY").is_some() {
        targets.extend(
            DEFAULT_OPENAI_TARGETS
                .iter()
                .map(|(label, model)| ExecutionTarget::new(*label, "openai", *model)),
        );
    }

    targets
}

fn uses_ollama_targets(targets: &[ExecutionTarget]) -> bool {
    targets.iter().any(|target| target.provider == "ollama")
}
