use std::sync::Arc;

use agents::agent::{
    Agent, AgentError, AgentEvent, AgentInput, AgentRunInput, AgentRunOutput, ExecutionProfile,
    SessionAgent,
};
use agents::llm::LlmRunner;
use agents::llm::completion::InputItem;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::{EvalError, EvalResult};
use crate::eval::EvalContext;
use crate::grade::{GradeResult, Grader, predicate};
use crate::trial::{AgentTrial, RecordedEvent};

const DEFAULT_JUDGE_PROMPT: &str = "You are an evaluation judge. Read the rubric and transcript carefully. Return a JSON verdict with score in [0.0, 1.0], a short summary, and an `evidence` array of short strings. Score 1.0 only when the assistant fully satisfies the rubric. Score 0.0 when it clearly fails. Use intermediate values only when the result is partially correct.";

/// Input sent to the built-in judge agent.
///
/// This is assembled automatically by [`judge`]. Most authored code does not
/// construct `JudgeInput` by hand.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JudgeInput<Output> {
    pub rubric: String,
    pub suite_id: String,
    pub eval_id: String,
    pub transcript: Vec<RecordedEvent>,
    pub final_reply: Option<Output>,
}

impl<Output> From<JudgeInput<Output>> for InputItem
where
    Output: Clone + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static,
{
    fn from(value: JudgeInput<Output>) -> Self {
        InputItem::user_text(
            serde_json::to_string_pretty(&value).expect("serialize judge input to JSON"),
        )
    }
}

/// Verdict returned by the built-in judge agent.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JudgeVerdict {
    pub score: f32,
    pub summary: String,
    #[serde(default)]
    pub evidence: Vec<String>,
}

/// Built-in LLM-backed grader agent used by [`judge`].
///
/// Most eval authors should use [`judge`] rather than instantiating
/// `JudgeAgent` directly.
pub struct JudgeAgent<Output>
where
    Output: Clone + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static,
{
    inner: SessionAgent<JudgeInput<Output>, (), (), JudgeVerdict>,
}

impl<Output> JudgeAgent<Output>
where
    Output: Clone + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static,
{
    /// Builds the built-in judge agent on top of the provided runner.
    pub fn new(runner: Arc<LlmRunner>) -> EvalResult<Self> {
        let inner = SessionAgent::builder()
            .with_llm_runner(runner)
            .with_execution_profile(ExecutionProfile::deterministic())
            .with_context_manager(agents::agent::ContextManager::static_text(
                DEFAULT_JUDGE_PROMPT,
            ))
            .with_message_type::<JudgeInput<Output>>()
            .with_response_type::<JudgeVerdict>()
            .build()
            .map_err(|error| EvalError::message(error.to_string()))?;

        Ok(Self { inner })
    }
}

#[async_trait]
impl<Output> Agent for JudgeAgent<Output>
where
    Output: Clone + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static,
{
    type Input = JudgeInput<Output>;
    type ToolCall = ();
    type ToolResult = ();
    type Output = JudgeVerdict;

    async fn send(&mut self, input: AgentInput<Self::Input>) -> Result<(), AgentError> {
        self.inner.send(input).await
    }

    async fn next(
        &mut self,
    ) -> Result<Option<AgentEvent<Self::ToolCall, Self::ToolResult, Self::Output>>, AgentError>
    {
        self.inner.next().await
    }

    async fn spawn(
        self,
    ) -> Result<
        (
            AgentRunInput<Self::Input>,
            AgentRunOutput<Self::ToolCall, Self::ToolResult, Self::Output>,
        ),
        AgentError,
    > {
        self.inner.spawn().await
    }
}

/// Creates an LLM-backed grader from a natural-language rubric.
///
/// Use `judge` when the score should be produced by another model reading the
/// transcript and final reply.
///
/// ```rust
/// use evals::{Grader, judge};
///
/// let grader: Grader<(), String> = judge(
///     "helpfulness",
///     "Did the assistant answer the user's request accurately and directly?",
/// );
/// ```
pub fn judge<State, Output>(
    name: impl Into<String>,
    rubric: impl Into<String>,
) -> Grader<State, Output>
where
    State: Send + Sync + 'static,
    Output: Clone + Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static,
{
    let rubric = rubric.into();
    predicate(
        name,
        move |trial: AgentTrial<Output>, ctx: EvalContext<State>| {
            let rubric = rubric.clone();
            async move {
                let input = JudgeInput {
                    rubric,
                    suite_id: ctx.suite_id.clone(),
                    eval_id: ctx.eval_id.clone(),
                    transcript: trial.transcript.clone(),
                    final_reply: trial.final_reply.clone(),
                };

                let mut judge = JudgeAgent::new(ctx.llm_runner())?;
                let verdict = judge
                    .call(input)
                    .await
                    .map_err(|error| EvalError::message(error.to_string()))?;

                Ok(GradeResult {
                    score: verdict.score,
                    summary: verdict.summary,
                    evidence: verdict.evidence.into(),
                })
            }
        },
    )
}
