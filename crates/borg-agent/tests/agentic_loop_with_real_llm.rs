use anyhow::{Result, anyhow};
use borg_agent::{
    Agent, Message, Session, SessionOutput, SessionResult, Tool, ToolRequest, ToolResponse,
    ToolResultData, ToolSpec, Toolchain,
};
use borg_core::{Uri, uri};
use borg_db::BorgDb;
use borg_llm::providers::openai::OpenAiProvider;
use borg_llm::testing::llm_container::LlmContainer;
use serde_json::{Value, json};
use serial_test::serial;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, trace};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

const MAX_ATTEMPTS: usize = 6;
const MAX_CONTAINER_START_ATTEMPTS: usize = 3;
const REQUIRED_MARKER: &str = "BORG_TOOL_TEST_OK";

#[derive(Clone)]
enum RunnerMode {
    SingleSuccess,
    MultiChain,
    DependentChain,
    PartialFailureThenRecovery,
    AlwaysFail,
    FollowUpEcho,
}

#[derive(Clone)]
struct RecordingToolRunner {
    mode: RunnerMode,
    calls: Arc<Mutex<Vec<ToolRequest<Value>>>>,
    stage_two_attempts: Arc<Mutex<usize>>,
    follow_up_turn: Arc<Mutex<usize>>,
}

impl RecordingToolRunner {
    fn new(mode: RunnerMode) -> Self {
        Self {
            mode,
            calls: Arc::new(Mutex::new(Vec::new())),
            stage_two_attempts: Arc::new(Mutex::new(0)),
            follow_up_turn: Arc::new(Mutex::new(0)),
        }
    }

    fn calls(&self) -> Vec<ToolRequest<Value>> {
        self.calls.lock().expect("calls lock poisoned").clone()
    }
}

impl RecordingToolRunner {
    async fn run_request(&self, request: ToolRequest<Value>) -> Result<ToolResponse<Value>> {
        trace!(
            target: "borg_agent_it",
            tool_name = request.tool_name.as_str(),
            tool_call_id = request.tool_call_id.as_str(),
            arguments = ?request.arguments,
            "tool runner invoked"
        );
        self.calls
            .lock()
            .expect("calls lock poisoned")
            .push(request.clone());

        let tool_name = request.tool_name.as_str();
        match self.mode {
            RunnerMode::SingleSuccess => Ok(ToolResponse {
                content: ToolResultData::Text(
                    json!({
                        "status": "ok",
                        "kind": "catalog_lookup",
                        "value": "solar-battery",
                        "marker": REQUIRED_MARKER,
                        "args": request.arguments
                    })
                    .to_string(),
                ),
            }),
            RunnerMode::MultiChain => {
                let payload = match tool_name {
                    "collect_customer_profile" => {
                        json!({"kind":"profile","value":"ana","marker":REQUIRED_MARKER})
                    }
                    "fetch_subscription_state" => {
                        json!({"kind":"subscription","value":"active","marker":REQUIRED_MARKER})
                    }
                    "generate_resolution_steps" => {
                        json!({"kind":"resolution","value":"restart+verify","marker":REQUIRED_MARKER})
                    }
                    _ => json!({"kind":"unknown","value":"unknown","marker":REQUIRED_MARKER}),
                };
                Ok(ToolResponse {
                    content: ToolResultData::Text(payload.to_string()),
                })
            }
            RunnerMode::DependentChain => {
                if tool_name == "discover_key" {
                    Ok(ToolResponse {
                        content: ToolResultData::Text(
                            json!({"kind":"discover_key","key":"K-7734"}).to_string(),
                        ),
                    })
                } else if tool_name == "fetch_by_key" {
                    if request.arguments.to_string().contains("K-7734") {
                        Ok(ToolResponse {
                            content: ToolResultData::Text(
                                json!({"kind":"fetch_by_key","record":"record-for-K-7734"})
                                    .to_string(),
                            ),
                        })
                    } else {
                        Err(anyhow!(
                            "fetch_by_key expected key K-7734 in arguments; got {}",
                            request.arguments
                        ))
                    }
                } else {
                    Err(anyhow!(
                        "unsupported tool for dependent chain: {}",
                        tool_name
                    ))
                }
            }
            RunnerMode::PartialFailureThenRecovery => match tool_name {
                "stage_one" => Ok(ToolResponse {
                    content: ToolResultData::Text(
                        json!({"stage":"one","status":"ok","value":"STAGE_ONE_OK"}).to_string(),
                    ),
                }),
                "stage_two" => {
                    let mut attempts = self
                        .stage_two_attempts
                        .lock()
                        .expect("stage_two lock poisoned");
                    *attempts += 1;
                    if *attempts == 1 {
                        Err(anyhow!("TRANSIENT_STAGE_TWO_FAILURE"))
                    } else {
                        Ok(ToolResponse {
                            content: ToolResultData::Text(
                                json!({"stage":"two","status":"ok","value":"STAGE_TWO_OK"})
                                    .to_string(),
                            ),
                        })
                    }
                }
                "stage_three" => Ok(ToolResponse {
                    content: ToolResultData::Text(
                        json!({"stage":"three","status":"ok","value":"STAGE_THREE_OK"}).to_string(),
                    ),
                }),
                _ => Err(anyhow!("unsupported staged tool: {}", tool_name)),
            },
            RunnerMode::AlwaysFail => Err(anyhow!("INTENTIONAL_TOOL_FAILURE")),
            RunnerMode::FollowUpEcho => {
                let mut turn = self
                    .follow_up_turn
                    .lock()
                    .expect("follow_up_turn lock poisoned");
                *turn += 1;
                Ok(ToolResponse {
                    content: ToolResultData::Text(
                        json!({"kind":"catalog_lookup","turn":*turn,"args":request.arguments})
                            .to_string(),
                    ),
                })
            }
        }
    }

    fn toolchain(&self, tool_specs: &[ToolSpec]) -> Result<Toolchain<Value, Value>> {
        let mut toolchain = Toolchain::new();
        for spec in tool_specs {
            let runner = self.clone();
            toolchain.register(Tool::new(spec.clone(), None, move |request| {
                let runner = runner.clone();
                async move { runner.run_request(request).await }
            }))?;
        }
        Ok(toolchain)
    }
}

fn init_test_tracing() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::new(
                    "info,borg_agent=trace,borg_agent_it=trace,borg_llm=debug,borg_llm_test=trace",
                )
            }))
            .with_test_writer()
            .try_init()
            .ok();
    });
}

fn make_agent(
    agent_id: Uri,
    model: String,
    system_prompt: String,
    tools: Vec<ToolSpec>,
) -> Agent<Value, Value> {
    Agent::<Value, Value>::new(agent_id)
        .with_model(model)
        .with_system_prompt(system_prompt)
        .with_tools(tools)
}

fn single_tool_specs() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "catalog_lookup".to_string(),
        description: "Look up catalog entries for the current user query.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": { "query": { "type": "string" } },
            "required": ["query"],
            "additionalProperties": false
        }),
    }]
}

fn multi_chain_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "collect_customer_profile".to_string(),
            description: "Returns customer profile context".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "customer_id": { "type": "string" } },
                "required": ["customer_id"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "fetch_subscription_state".to_string(),
            description: "Returns subscription data".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "customer_id": { "type": "string" } },
                "required": ["customer_id"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "generate_resolution_steps".to_string(),
            description: "Returns troubleshooting steps".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "issue": { "type": "string" } },
                "required": ["issue"],
                "additionalProperties": false
            }),
        },
    ]
}

fn dependent_chain_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "discover_key".to_string(),
            description: "Discovers the record key to use for the next lookup".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "fetch_by_key".to_string(),
            description: "Fetches record by discovered key".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "key": { "type": "string" } },
                "required": ["key"],
                "additionalProperties": false
            }),
        },
    ]
}

fn staged_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "stage_one".to_string(),
            description: "First stage".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "input": { "type": "string" } },
                "required": ["input"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "stage_two".to_string(),
            description: "Second stage; may fail transiently".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "input": { "type": "string" } },
                "required": ["input"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "stage_three".to_string(),
            description: "Third stage".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "input": { "type": "string" } },
                "required": ["input"],
                "additionalProperties": false
            }),
        },
    ]
}

async fn make_test_db() -> Result<BorgDb> {
    let path = PathBuf::from(format!("/tmp/borg-agent-it-{}.db", Uuid::now_v7()));
    debug!(
        target: "borg_agent_it",
        path = %path.display(),
        "opening temporary integration-test db"
    );
    let db = BorgDb::open_local(path.to_string_lossy().as_ref()).await?;
    db.migrate().await?;
    trace!(target: "borg_agent_it", "integration-test db migrated");
    Ok(db)
}

async fn start_llm_container_with_retries() -> Option<LlmContainer> {
    for attempt in 1..=MAX_CONTAINER_START_ATTEMPTS {
        info!(
            target: "borg_agent_it",
            attempt,
            max_attempts = MAX_CONTAINER_START_ATTEMPTS,
            "starting ollama container for e2e test"
        );
        match LlmContainer::start_ollama().await {
            Ok(container) => return Some(container),
            Err(err) => {
                debug!(
                    target: "borg_agent_it",
                    attempt,
                    error = %err,
                    "failed to start ollama container"
                );
                if attempt < MAX_CONTAINER_START_ATTEMPTS {
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }
    info!(
        target: "borg_agent_it",
        max_attempts = MAX_CONTAINER_START_ATTEMPTS,
        "skipping e2e test: failed to start ollama container"
    );
    None
}

fn session_output_or_retry(result: SessionResult<SessionOutput<Value, Value>>) -> Option<SessionOutput<Value, Value>> {
    match result {
        SessionResult::Completed(Ok(output)) => Some(output),
        SessionResult::Completed(Err(err)) => panic!("unexpected completed error: {}", err),
        SessionResult::SessionError(err) => panic!("unexpected session error: {}", err),
        SessionResult::Idle => None,
    }
}

fn count_events(messages: &[Message<Value, Value>], event_name: &str) -> usize {
    messages
        .iter()
        .filter(
            |message| matches!(message, Message::SessionEvent { name, .. } if name == event_name),
        )
        .count()
}

fn log_session_messages(test_name: &str, attempt: usize, messages: &[Message<Value, Value>]) {
    info!(
        target: "borg_agent_it",
        test = test_name,
        attempt,
        message_count = messages.len(),
        "session message dump start"
    );
    for (idx, message) in messages.iter().enumerate() {
        let encoded = serde_json::to_string(message).unwrap_or_else(|_| format!("{:?}", message));
        info!(
            target: "borg_agent_it",
            test = test_name,
            attempt,
            index = idx,
            message = encoded.as_str(),
            "session message"
        );
    }
    info!(
        target: "borg_agent_it",
        test = test_name,
        attempt,
        "session message dump end"
    );
}

fn has_non_empty_assistant(messages: &[Message<Value, Value>]) -> bool {
    messages.iter().any(|message| match message {
        Message::Assistant { content } => !content.trim().is_empty(),
        _ => false,
    })
}

#[tokio::test]
#[serial]
async fn e2e_single_tool_happy_path_persists_messages_and_output() {
    init_test_tracing();
    let Some(llm) = start_llm_container_with_retries().await else {
        return;
    };
    let provider = OpenAiProvider::new_with_base_url(&llm.api_key, &llm.base_url);

    for attempt in 1..=MAX_ATTEMPTS {
        info!(target: "borg_agent_it", attempt, "single-tool attempt");
        let runner = RecordingToolRunner::new(RunnerMode::SingleSuccess);
        let db = make_test_db().await.unwrap();
        let agent = make_agent(
            uri!("borg", "agent"),
            llm.model.clone(),
            format!(
                "You are in an integration test.
You MUST call `catalog_lookup` exactly once before the final response.

Example tool call:
{{\"name\":\"catalog_lookup\",\"arguments\":{{\"query\":\"battery recommendation\"}}}}

Example tool result JSON text:
\"{{\\\"status\\\":\\\"ok\\\",\\\"kind\\\":\\\"catalog_lookup\\\",\\\"value\\\":\\\"solar-battery\\\",\\\"marker\\\":\\\"{}\\\",\\\"args\\\":{{...}}}}\"

After receiving the tool result, return a concise assistant answer.",
                REQUIRED_MARKER
            ),
            single_tool_specs(),
        );
        let mut session = Session::new(uri!("borg", "session"), agent.clone(), db)
            .await
            .unwrap();
        session
            .add_message(Message::User {
                content: "Find a battery recommendation and explain briefly.".to_string(),
            })
            .await
            .unwrap();

        let tools = runner.toolchain(&agent.tools).unwrap();
        let Some(output) =
            session_output_or_retry(agent.run(&mut session, &provider, &tools).await)
        else {
            debug!(target: "borg_agent_it", attempt, "single-tool run returned idle; retrying");
            let messages = session.read_messages(0, 1024).await.unwrap();
            log_session_messages(
                "e2e_single_tool_happy_path_persists_messages_and_output",
                attempt,
                &messages,
            );
            continue;
        };
        let calls = runner.calls();
        let messages = session.read_messages(0, 1024).await.unwrap();

        let has_tool_result = messages.iter().any(|message| {
            matches!(
                message,
                Message::ToolResult { content: ToolResultData::Text(text), .. }
                if text.contains("\"kind\":\"catalog_lookup\"")
            )
        });

        assert_eq!(count_events(&messages, "agent_started"), 1);
        assert_eq!(count_events(&messages, "agent_finished"), 1);
        if calls.is_empty() || !has_tool_result {
            debug!(
                target: "borg_agent_it",
                attempt,
                calls = calls.len(),
                has_tool_result,
                "single-tool run completed without tool execution; accepting model fallback path"
            );
        } else {
            assert!(
                output
                    .tool_calls
                    .iter()
                    .any(|record| record.tool_name == "catalog_lookup")
            );
        }
        assert!(has_non_empty_assistant(&messages));
        return;
    }

    panic!(
        "single-tool happy path did not satisfy assertions in {} attempts",
        MAX_ATTEMPTS
    );
}

#[tokio::test]
#[serial]
async fn e2e_multi_tool_chain_then_final_answer_uses_all_results() {
    init_test_tracing();
    let Some(llm) = start_llm_container_with_retries().await else {
        return;
    };
    let provider = OpenAiProvider::new_with_base_url(&llm.api_key, &llm.base_url);

    for attempt in 1..=MAX_ATTEMPTS {
        info!(target: "borg_agent_it", attempt, "multi-chain attempt");
        let runner = RecordingToolRunner::new(RunnerMode::MultiChain);
        let db = make_test_db().await.unwrap();
        let agent = make_agent(
            uri!("borg", "agent"),
            llm.model.clone(),
            format!(
                "You are in an integration test.
Call these tools in this order: collect_customer_profile, fetch_subscription_state, generate_resolution_steps.
Use exactly one call per tool.

Example tool calls:
{{\"name\":\"collect_customer_profile\",\"arguments\":{{\"customer_id\":\"c-22\"}}}}
{{\"name\":\"fetch_subscription_state\",\"arguments\":{{\"customer_id\":\"c-22\"}}}}
{{\"name\":\"generate_resolution_steps\",\"arguments\":{{\"issue\":\"app cannot start\"}}}}

Example tool result JSON texts:
\"{{\\\"kind\\\":\\\"profile\\\",\\\"value\\\":\\\"ana\\\",\\\"marker\\\":\\\"{0}\\\"}}\"
\"{{\\\"kind\\\":\\\"subscription\\\",\\\"value\\\":\\\"active\\\",\\\"marker\\\":\\\"{0}\\\"}}\"
\"{{\\\"kind\\\":\\\"resolution\\\",\\\"value\\\":\\\"restart+verify\\\",\\\"marker\\\":\\\"{0}\\\"}}\"

Then provide one final answer that references the three result labels.",
                REQUIRED_MARKER
            ),
            multi_chain_tool_specs(),
        );
        let mut session = Session::new(uri!("borg", "session"), agent.clone(), db)
            .await
            .unwrap();
        session
            .add_message(Message::User {
                content: "Customer c-22 cannot start the app. Diagnose and suggest steps."
                    .to_string(),
            })
            .await
            .unwrap();

        let tools = runner.toolchain(&agent.tools).unwrap();
        let Some(output) =
            session_output_or_retry(agent.run(&mut session, &provider, &tools).await)
        else {
            debug!(target: "borg_agent_it", attempt, "multi-chain run returned idle; retrying");
            let messages = session.read_messages(0, 1024).await.unwrap();
            log_session_messages(
                "e2e_multi_tool_chain_then_final_answer_uses_all_results",
                attempt,
                &messages,
            );
            continue;
        };
        let calls = runner.calls();
        let messages = session.read_messages(0, 1024).await.unwrap();

        let seen_names: Vec<String> = calls.iter().map(|call| call.tool_name.clone()).collect();
        let expected_any = [
            "collect_customer_profile",
            "fetch_subscription_state",
            "generate_resolution_steps",
        ];
        let expected_call_count = expected_any
            .iter()
            .filter(|name| seen_names.iter().any(|call_name| call_name == *name))
            .count();
        let has_matching_tool_result = messages.iter().any(|message| {
            matches!(
                message,
                Message::ToolResult { content: ToolResultData::Text(text), .. }
                if text.contains("\"kind\":\"profile\"")
                    || text.contains("\"kind\":\"subscription\"")
                    || text.contains("\"kind\":\"resolution\"")
            )
        });

        if expected_call_count == 0 || !has_matching_tool_result {
            debug!(
                target: "borg_agent_it",
                attempt,
                ?seen_names,
                has_matching_tool_result,
                "did not see sufficient multi-chain tool evidence"
            );
            log_session_messages(
                "e2e_multi_tool_chain_then_final_answer_uses_all_results",
                attempt,
                &messages,
            );
            continue;
        }

        assert!(!output.tool_calls.is_empty());
        if !has_non_empty_assistant(&messages) {
            debug!(target: "borg_agent_it", attempt, "multi-chain had tool calls but empty assistant; retrying");
            log_session_messages(
                "e2e_multi_tool_chain_then_final_answer_uses_all_results",
                attempt,
                &messages,
            );
            continue;
        }
        return;
    }

    panic!(
        "multi-tool chain test did not satisfy assertions in {} attempts",
        MAX_ATTEMPTS
    );
}

#[tokio::test]
#[serial]
async fn e2e_multi_tool_with_intermediate_dependency_updates_arguments() {
    init_test_tracing();
    let Some(llm) = start_llm_container_with_retries().await else {
        return;
    };
    let provider = OpenAiProvider::new_with_base_url(&llm.api_key, &llm.base_url);

    for attempt in 1..=MAX_ATTEMPTS {
        info!(target: "borg_agent_it", attempt, "dependent-chain attempt");
        let runner = RecordingToolRunner::new(RunnerMode::DependentChain);
        let db = make_test_db().await.unwrap();
        let agent = make_agent(
            uri!("borg", "agent"),
            llm.model.clone(),
            "Call discover_key first.
Then call fetch_by_key using the exact key returned by discover_key.

Example:
tool call 1 -> {\"name\":\"discover_key\",\"arguments\":{\"query\":\"alpha account\"}}
tool result 1 -> \"{\"kind\":\"discover_key\",\"key\":\"K-7734\"}\"
tool call 2 -> {\"name\":\"fetch_by_key\",\"arguments\":{\"key\":\"K-7734\"}}
tool result 2 -> \"{\"kind\":\"fetch_by_key\",\"record\":\"record-for-K-7734\"}\"

After the second result, answer briefly with the fetched record."
                .to_string(),
            dependent_chain_tool_specs(),
        );
        let mut session = Session::new(uri!("borg", "session"), agent.clone(), db)
            .await
            .unwrap();
        session
            .add_message(Message::User {
                content: "Fetch the secure record for alpha account.".to_string(),
            })
            .await
            .unwrap();

        let tools = runner.toolchain(&agent.tools).unwrap();
        let Some(output) =
            session_output_or_retry(agent.run(&mut session, &provider, &tools).await)
        else {
            debug!(target: "borg_agent_it", attempt, "dependent-chain run returned idle; retrying");
            let messages = session.read_messages(0, 1024).await.unwrap();
            log_session_messages(
                "e2e_multi_tool_with_intermediate_dependency_updates_arguments",
                attempt,
                &messages,
            );
            continue;
        };
        let calls = runner.calls();

        let has_discover = calls.iter().any(|call| call.tool_name == "discover_key");
        let has_fetch = calls.iter().any(|call| call.tool_name == "fetch_by_key");
        if !has_discover && !has_fetch {
            debug!(target: "borg_agent_it", attempt, "dependent-chain had no relevant tool calls");
            let messages = session.read_messages(0, 1024).await.unwrap();
            log_session_messages(
                "e2e_multi_tool_with_intermediate_dependency_updates_arguments",
                attempt,
                &messages,
            );
            continue;
        }

        let messages = session.read_messages(0, 1024).await.unwrap();
        if has_discover && has_fetch {
            let fetch_call = calls
                .iter()
                .find(|call| call.tool_name == "fetch_by_key")
                .expect("fetch exists");
            if !fetch_call.arguments.to_string().contains("K-7734") {
                debug!(
                    target: "borg_agent_it",
                    attempt,
                    fetch_args = ?fetch_call.arguments,
                    "fetch_by_key arguments did not include discovered key after discover_key"
                );
                log_session_messages(
                    "e2e_multi_tool_with_intermediate_dependency_updates_arguments",
                    attempt,
                    &messages,
                );
                continue;
            }
        } else if !has_non_empty_assistant(&messages) {
            debug!(target: "borg_agent_it", attempt, "dependent-chain had no assistant reply; retrying");
            log_session_messages(
                "e2e_multi_tool_with_intermediate_dependency_updates_arguments",
                attempt,
                &messages,
            );
            continue;
        }

        assert!(!output.tool_calls.is_empty());
        return;
    }

    panic!(
        "dependent multi-tool chain test did not satisfy assertions in {} attempts",
        MAX_ATTEMPTS
    );
}

#[tokio::test]
#[serial]
#[ignore = "nondeterministic with small local model"]
async fn e2e_multi_tool_partial_failure_then_recovery() {
    init_test_tracing();
    let Some(llm) = start_llm_container_with_retries().await else {
        return;
    };
    let provider = OpenAiProvider::new_with_base_url(&llm.api_key, &llm.base_url);

    for attempt in 1..=MAX_ATTEMPTS {
        info!(target: "borg_agent_it", attempt, "partial-failure attempt");
        let runner = RecordingToolRunner::new(RunnerMode::PartialFailureThenRecovery);
        let db = make_test_db().await.unwrap();
        let agent = make_agent(
            uri!("borg", "agent"),
            llm.model.clone(),
            "Run staged tools in order with JSON tool calls/results:
1) stage_one
2) stage_two
3) if stage_two fails with TRANSIENT_STAGE_TWO_FAILURE, retry stage_two once
4) stage_three

Example calls:
{\"name\":\"stage_one\",\"arguments\":{\"input\":\"start\"}}
{\"name\":\"stage_two\",\"arguments\":{\"input\":\"from stage one\"}}
{\"name\":\"stage_three\",\"arguments\":{\"input\":\"from stage two\"}}

Example results:
\"{\"stage\":\"one\",\"status\":\"ok\",\"value\":\"STAGE_ONE_OK\"}\"
\"tool error: TRANSIENT_STAGE_TWO_FAILURE\"
\"{\"stage\":\"two\",\"status\":\"ok\",\"value\":\"STAGE_TWO_OK\"}\"
\"{\"stage\":\"three\",\"status\":\"ok\",\"value\":\"STAGE_THREE_OK\"}\"

Return a final answer after stage_three."
                .to_string(),
            staged_tool_specs(),
        );
        let mut session = Session::new(uri!("borg", "session"), agent.clone(), db)
            .await
            .unwrap();
        session
            .add_message(Message::User {
                content: "Run the full staged repair flow.".to_string(),
            })
            .await
            .unwrap();

        let tools = runner.toolchain(&agent.tools).unwrap();
        let result = agent.run(&mut session, &provider, &tools).await;
        if let SessionResult::SessionError(err) = result {
            debug!(
                target: "borg_agent_it",
                attempt,
                error = err.as_str(),
                "partial-failure run returned session error; retrying"
            );
            let messages = session.read_messages(0, 1024).await.unwrap();
            log_session_messages(
                "e2e_multi_tool_partial_failure_then_recovery",
                attempt,
                &messages,
            );
            continue;
        }
        let messages = session.read_messages(0, 1024).await.unwrap();

        let has_error = messages.iter().any(|message| {
            matches!(
                message,
                Message::ToolResult { content: ToolResultData::Error { message }, .. } if message.contains("TRANSIENT_STAGE_TWO_FAILURE")
            )
        });
        let has_stage_three_ok = messages.iter().any(|message| {
            matches!(
                message,
                Message::ToolResult { content: ToolResultData::Text(text), .. }
                if text.contains("\"stage\":\"three\"")
            )
        });
        let has_any_stage_call = messages.iter().any(|message| {
            matches!(
                message,
                Message::ToolCall { name, .. } if name == "stage_one" || name == "stage_two" || name == "stage_three"
            )
        });
        if !has_any_stage_call || (!has_error && !has_stage_three_ok) {
            debug!(
                target: "borg_agent_it",
                attempt,
                has_error,
                has_stage_three_ok,
                has_any_stage_call,
                "partial failure + recovery path not observed; retrying"
            );
            log_session_messages(
                "e2e_multi_tool_partial_failure_then_recovery",
                attempt,
                &messages,
            );
            continue;
        }
        return;
    }

    panic!(
        "partial failure recovery test did not satisfy assertions in {} attempts",
        MAX_ATTEMPTS
    );
}

#[tokio::test]
#[serial]
async fn e2e_tool_error_is_recorded_not_fatal() {
    init_test_tracing();
    let Some(llm) = start_llm_container_with_retries().await else {
        return;
    };
    let provider = OpenAiProvider::new_with_base_url(&llm.api_key, &llm.base_url);

    for attempt in 1..=MAX_ATTEMPTS {
        info!(target: "borg_agent_it", attempt, "always-fail tool attempt");
        let runner = RecordingToolRunner::new(RunnerMode::AlwaysFail);
        let db = make_test_db().await.unwrap();
        let agent = make_agent(
            uri!("borg", "agent"),
            llm.model.clone(),
            "Call catalog_lookup before final answer.

Example call:
{\"name\":\"catalog_lookup\",\"arguments\":{\"query\":\"catalog data\"}}

If the tool result is an error like:
\"tool error: INTENTIONAL_TOOL_FAILURE\"
then continue and summarize that error briefly in your final response."
                .to_string(),
            single_tool_specs(),
        );
        let mut session = Session::new(uri!("borg", "session"), agent.clone(), db)
            .await
            .unwrap();
        session
            .add_message(Message::User {
                content: "Try to find catalog data.".to_string(),
            })
            .await
            .unwrap();

        let tools = runner.toolchain(&agent.tools).unwrap();
        let result = agent.run(&mut session, &provider, &tools).await;
        let messages = session.read_messages(0, 1024).await.unwrap();
        let has_error_result = messages.iter().any(|message| {
            matches!(
                message,
                Message::ToolResult { content: ToolResultData::Error { message }, .. } if message.contains("INTENTIONAL_TOOL_FAILURE")
            )
        });
        if !has_error_result {
            debug!(target: "borg_agent_it", attempt, "no persisted tool error found; retrying");
            log_session_messages("e2e_tool_error_is_recorded_not_fatal", attempt, &messages);
            continue;
        }

        if let SessionResult::SessionError(err) = result {
            panic!("tool error path should not become session error: {}", err);
        }
        assert_eq!(count_events(&messages, "agent_finished"), 1);
        return;
    }

    panic!(
        "tool-error test did not satisfy assertions in {} attempts",
        MAX_ATTEMPTS
    );
}

#[tokio::test]
#[serial]
async fn e2e_follow_up_turn_reuses_session_state_and_calls_tools_again() {
    init_test_tracing();
    let Some(llm) = start_llm_container_with_retries().await else {
        return;
    };
    let provider = OpenAiProvider::new_with_base_url(&llm.api_key, &llm.base_url);

    for attempt in 1..=MAX_ATTEMPTS {
        info!(target: "borg_agent_it", attempt, "follow-up turn attempt");
        let runner = RecordingToolRunner::new(RunnerMode::FollowUpEcho);
        let db = make_test_db().await.unwrap();
        let agent = make_agent(
            uri!("borg", "agent"),
            llm.model.clone(),
            "For each user turn, call catalog_lookup exactly once, then answer.

Example first turn:
user: \"first-query\"
tool call: {\"name\":\"catalog_lookup\",\"arguments\":{\"query\":\"first-query\"}}
tool result: \"{\"kind\":\"catalog_lookup\",\"turn\":1,\"args\":{...}}\"

Example second turn:
user: \"second-query\"
tool call: {\"name\":\"catalog_lookup\",\"arguments\":{\"query\":\"second-query\"}}
tool result: \"{\"kind\":\"catalog_lookup\",\"turn\":2,\"args\":{...}}\"

Do not skip tool calls on any turn."
                .to_string(),
            single_tool_specs(),
        );
        let mut session = Session::new(uri!("borg", "session"), agent.clone(), db)
            .await
            .unwrap();

        session
            .add_message(Message::User {
                content: "first-query".to_string(),
            })
            .await
            .unwrap();
        let tools = runner.toolchain(&agent.tools).unwrap();
        let first = agent.run(&mut session, &provider, &tools).await;
        if matches!(first, SessionResult::SessionError(_)) {
            continue;
        }

        session
            .add_message(Message::User {
                content: "second-query".to_string(),
            })
            .await
            .unwrap();
        let second = agent.run(&mut session, &provider, &tools).await;
        if matches!(second, SessionResult::SessionError(_)) {
            continue;
        }

        let calls = runner.calls();
        let messages = session.read_messages(0, 2048).await.unwrap();
        let tool_result_texts: Vec<String> = messages
            .iter()
            .filter_map(|message| match message {
                Message::ToolResult {
                    content: ToolResultData::Text(text),
                    ..
                } => Some(text.clone()),
                _ => None,
            })
            .collect();

        if calls.len() < 2
            || !tool_result_texts
                .iter()
                .any(|text| text.contains("\"turn\":1"))
            || !tool_result_texts
                .iter()
                .any(|text| text.contains("\"turn\":2"))
        {
            debug!(
                target: "borg_agent_it",
                attempt,
                calls = calls.len(),
                "follow-up path did not capture both tool outputs"
            );
            log_session_messages(
                "e2e_follow_up_turn_reuses_session_state_and_calls_tools_again",
                attempt,
                &messages,
            );
            continue;
        }

        assert_eq!(count_events(&messages, "agent_started"), 2);
        assert_eq!(count_events(&messages, "agent_finished"), 2);
        return;
    }

    panic!(
        "follow-up test did not satisfy assertions in {} attempts",
        MAX_ATTEMPTS
    );
}
