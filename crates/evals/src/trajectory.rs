use std::collections::BTreeMap;
use std::{future::Future, pin::Pin};

use agents::agent::{Agent, AgentEvent, AgentInput};
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::error::EvalError;
use crate::eval::EvalContext;
use crate::grade::GradingConfig;
use crate::trial::{AgentTrial, RecordedEvent, TranscriptAgent, TranscriptCollector};

fn event_kind<Tool, ToolResult, Output>(
    event: &AgentEvent<Tool, ToolResult, Output>,
) -> &'static str {
    match event {
        AgentEvent::ContextWindowMaterialized { .. } => "context_window_materialized",
        AgentEvent::RequestPrepared { .. } => "request_prepared",
        AgentEvent::ModelOutputItem { .. } => "model_output_item",
        AgentEvent::ToolCallRequested { .. } => "tool_call_requested",
        AgentEvent::ToolExecutionCompleted { .. } => "tool_execution_completed",
        AgentEvent::Completed { .. } => "completed",
        AgentEvent::Cancelled => "cancelled",
    }
}

/// One user step in a linear eval trajectory.
pub struct Step<A: Agent, State = ()> {
    user: A::Input,
    grade: Option<GradingConfig<State, A::Output>>,
}

/// A linear multi-step eval scenario for an agent.
pub struct Trajectory<A: Agent, State = ()> {
    steps: Vec<Step<A, State>>,
}

/// Builder for [`Trajectory`].
pub struct TrajectoryBuilder<A: Agent, State = ()> {
    steps: Vec<Step<A, State>>,
}

impl<A, State> Clone for Step<A, State>
where
    A: Agent,
    A::Input: Clone,
{
    fn clone(&self) -> Self {
        Self {
            user: self.user.clone(),
            grade: self.grade.clone(),
        }
    }
}

impl<A, State> Clone for Trajectory<A, State>
where
    A: Agent,
    A::Input: Clone,
{
    fn clone(&self) -> Self {
        Self {
            steps: self.steps.clone(),
        }
    }
}

impl<A, State> std::fmt::Debug for Step<A, State>
where
    A: Agent,
    A::Input: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Step")
            .field("user", &self.user)
            .field("grade", &self.grade)
            .finish()
    }
}

impl<A, State> std::fmt::Debug for Trajectory<A, State>
where
    A: Agent,
    A::Input: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Trajectory")
            .field("steps", &self.steps)
            .finish()
    }
}

impl<A, State> std::fmt::Debug for TrajectoryBuilder<A, State>
where
    A: Agent,
    A::Input: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrajectoryBuilder")
            .field("steps", &self.steps)
            .finish()
    }
}

impl<A: Agent, State> Step<A, State> {
    pub fn user(message: impl Into<A::Input>) -> Self {
        Self {
            user: message.into(),
            grade: None,
        }
    }

    pub fn grade<G>(mut self, grading: G) -> Self
    where
        G: Into<GradingConfig<State, A::Output>>,
    {
        self.grade = Some(grading.into());
        self
    }
}

impl<A: Agent, State> Trajectory<A, State> {
    pub fn new(step: Step<A, State>) -> Self {
        Self { steps: vec![step] }
    }

    pub fn builder() -> TrajectoryBuilder<A, State> {
        TrajectoryBuilder { steps: Vec::new() }
    }

    pub fn steps(&self) -> &[Step<A, State>] {
        &self.steps
    }
}

impl<A: Agent, State> TrajectoryBuilder<A, State> {
    pub fn add_step(mut self, step: Step<A, State>) -> Self {
        self.steps.push(step);
        self
    }

    pub fn build(self) -> Result<Trajectory<A, State>, EvalError> {
        if self.steps.is_empty() {
            return Err(EvalError::message(
                "trajectory must contain at least one step",
            ));
        }
        Ok(Trajectory { steps: self.steps })
    }
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;
type TrajectoryRunFuture<Output> = BoxFuture<Result<AgentTrial<Output>, EvalError>>;

impl<A, State> Trajectory<A, State>
where
    A: Agent,
    State: Send + Sync + 'static,
{
    pub fn runner(
        self,
    ) -> impl Fn(EvalContext<State>, A) -> TrajectoryRunFuture<A::Output> + Send + Sync + 'static
    {
        let trajectory = self;
        move |ctx, agent| {
            let trajectory = trajectory.clone();
            Box::pin(async move {
                info!(
                    suite_id = %ctx.suite_id,
                    eval_id = %ctx.eval_id,
                    trial_id = %ctx.trial_id,
                    trial_index = ctx.trial_index,
                    target_label = %ctx.target.label,
                    steps = trajectory.steps().len(),
                    "starting trajectory"
                );
                debug!(
                    suite_id = %ctx.suite_id,
                    eval_id = %ctx.eval_id,
                    trial_id = %ctx.trial_id,
                    trial_index = ctx.trial_index,
                    target_label = %ctx.target.label,
                    "starting agent run"
                );
                let (transcript_tx, mut transcript_collector) = TranscriptCollector::new(256);
                let agent = TranscriptAgent::new(agent, transcript_tx.clone());
                let (tx, mut rx) = agent
                    .spawn()
                    .await
                    .map_err(|error| EvalError::message(error.to_string()))?;
                debug!(
                    suite_id = %ctx.suite_id,
                    eval_id = %ctx.eval_id,
                    trial_id = %ctx.trial_id,
                    trial_index = ctx.trial_index,
                    target_label = %ctx.target.label,
                    "agent run started"
                );
                let mut collected_grades = BTreeMap::new();
                let mut collected_grader_failures = Vec::new();
                let mut final_reply = None;

                for (step_index, step) in trajectory.steps().iter().enumerate() {
                    debug!(
                        suite_id = %ctx.suite_id,
                        eval_id = %ctx.eval_id,
                        trial_id = %ctx.trial_id,
                        trial_index = ctx.trial_index,
                        target_label = %ctx.target.label,
                        step_index,
                        "sending trajectory step"
                    );
                    let _ = transcript_tx
                        .send(RecordedEvent::StepStarted {
                            step_index,
                            input: serde_json::to_value(&step.user)
                                .expect("serialize trajectory step input"),
                        })
                        .await;
                    if let Err(error) = tx.send(AgentInput::Message(step.user.clone())).await {
                        let transcript = transcript_collector.snapshot().await;
                        return Err(EvalError::message_with_trial(
                            format!("send trajectory step: {error}"),
                            AgentTrial::from_transcript(
                                transcript,
                                final_reply.clone(),
                                Value::Null,
                            ),
                        ));
                    }

                    let mut step_completed = false;
                    while let Some(event) = rx.recv().await {
                        match event {
                            Ok(event) => {
                                debug!(
                                    suite_id = %ctx.suite_id,
                                    eval_id = %ctx.eval_id,
                                    trial_id = %ctx.trial_id,
                                    trial_index = ctx.trial_index,
                                    target_label = %ctx.target.label,
                                    step_index,
                                    event_kind = event_kind(&event),
                                    "received trajectory event"
                                );
                                if let AgentEvent::Completed { reply, .. } = &event {
                                    final_reply = Some(reply.clone());
                                }
                                if matches!(event, AgentEvent::Completed { .. }) {
                                    step_completed = true;
                                    debug!(
                                        suite_id = %ctx.suite_id,
                                        eval_id = %ctx.eval_id,
                                        trial_id = %ctx.trial_id,
                                        trial_index = ctx.trial_index,
                                        target_label = %ctx.target.label,
                                    step_index,
                                        "trajectory step completed"
                                    );
                                    let _ = transcript_tx
                                        .send(RecordedEvent::StepCompleted { step_index })
                                        .await;
                                    break;
                                }
                            }
                            Err(error) => {
                                warn!(
                                    suite_id = %ctx.suite_id,
                                    eval_id = %ctx.eval_id,
                                    trial_id = %ctx.trial_id,
                                    trial_index = ctx.trial_index,
                                    target_label = %ctx.target.label,
                                    step_index,
                                    error = %error,
                                    "trajectory stream failed"
                                );
                                return Err(EvalError::message_with_trial(
                                    error.to_string(),
                                    AgentTrial::from_transcript(
                                        transcript_collector.snapshot().await,
                                        final_reply.clone(),
                                        Value::Null,
                                    ),
                                ));
                            }
                        }
                    }

                    if !step_completed {
                        warn!(
                            suite_id = %ctx.suite_id,
                            eval_id = %ctx.eval_id,
                            trial_id = %ctx.trial_id,
                            trial_index = ctx.trial_index,
                            target_label = %ctx.target.label,
                            step_index,
                            "trajectory step did not complete"
                        );
                        return Err(EvalError::message_with_trial(
                            "agent finished without completing a trajectory step",
                            AgentTrial::from_transcript(
                                transcript_collector.snapshot().await,
                                final_reply.clone(),
                                Value::Null,
                            ),
                        ));
                    }

                    if let Some(grading) = &step.grade {
                        debug!(
                            suite_id = %ctx.suite_id,
                            eval_id = %ctx.eval_id,
                            trial_id = %ctx.trial_id,
                            trial_index = ctx.trial_index,
                            target_label = %ctx.target.label,
                            step_index,
                            "running trajectory grade"
                        );
                        let snapshot = AgentTrial::from_transcript(
                            transcript_collector.snapshot().await,
                            final_reply.clone(),
                            Value::Null,
                        );
                        let outcome = grading
                            .run_with_scope(
                                snapshot.clone(),
                                ctx.clone(),
                                crate::trial::RecordedGradingScope::TrajectoryStep { step_index },
                            )
                            .await?;
                        for event in outcome.recorded_events.clone() {
                            let _ = transcript_tx.send(event).await;
                        }

                        collected_grades.extend(outcome.grades.clone());
                        collected_grader_failures.extend(outcome.grader_failures.clone());

                        if !outcome.passed {
                            warn!(
                                suite_id = %ctx.suite_id,
                                eval_id = %ctx.eval_id,
                                trial_id = %ctx.trial_id,
                                trial_index = ctx.trial_index,
                                target_label = %ctx.target.label,
                                step_index,
                                "trajectory grading failed"
                            );
                            let mut failed_snapshot = AgentTrial::from_transcript(
                                transcript_collector.snapshot().await,
                                final_reply.clone(),
                                Value::Null,
                            );
                            failed_snapshot.grades = collected_grades.clone();
                            failed_snapshot.grader_failures = collected_grader_failures.clone();
                            return Err(EvalError::message_with_trial(
                                format!("trajectory grading failed at step {step_index}"),
                                failed_snapshot,
                            ));
                        }
                    }
                }

                info!(
                    suite_id = %ctx.suite_id,
                    eval_id = %ctx.eval_id,
                    trial_id = %ctx.trial_id,
                    trial_index = ctx.trial_index,
                    target_label = %ctx.target.label,
                    "trajectory completed"
                );
                drop(tx);
                drop(transcript_tx);
                let transcript = transcript_collector.finish().await;
                let mut trial = AgentTrial::from_transcript(transcript, final_reply, Value::Null);
                trial.grades = collected_grades;
                trial.grader_failures = collected_grader_failures;
                Ok(trial)
            })
        }
    }
}
