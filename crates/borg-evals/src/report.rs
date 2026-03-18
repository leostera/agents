use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use borg_agent::Agent;
use chrono::SecondsFormat;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::ExecutionTarget;
use crate::error::EvalResult;
use crate::eval::Eval;
use crate::grade::{GradeResult, GraderFailure, is_passing_score};
use crate::suite::Suite;
use crate::trial::{AgentTrial, RecordedEvent, RecordedMessageRole};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunManifest {
    pub schema_version: u32,
    pub run_id: String,
    pub started_at: Duration,
    pub finished_at: Duration,
    pub suites: Vec<String>,
    pub targets: Vec<ExecutionTarget>,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EvalRunReport {
    pub manifest: RunManifest,
    pub variants: Vec<SuiteRunReport>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SuiteSummary {
    pub schema_version: u32,
    pub run_id: String,
    pub suite_id: String,
    pub target: ExecutionTarget,
    pub total_evals: usize,
    pub total_trials: usize,
    pub passed_trials: usize,
    pub pass_rate: f32,
    pub mean_score: f32,
    pub mean_duration: Duration,
    pub evals: Vec<EvalAggregate>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EvalAggregate {
    pub eval_id: String,
    pub trial_count: usize,
    pub passed_trials: usize,
    pub pass_rate: f32,
    pub mean_score: f32,
    pub mean_duration: Duration,
    pub grader_means: Vec<GraderAggregate>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct GraderAggregate {
    pub name: String,
    pub mean_score: f32,
    pub pass_rate: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TrialRecord {
    pub schema_version: u32,
    pub trial_id: String,
    pub run_id: String,
    pub suite_id: String,
    pub target: ExecutionTarget,
    pub eval_id: String,
    pub trial_index: usize,
    pub started_at: Duration,
    pub finished_at: Duration,
    pub duration: Duration,
    pub passed: bool,
    pub mean_score: f32,
    pub trial: Option<serde_json::Value>,
    pub error: Option<String>,
    pub grades: BTreeMap<String, GradeResult>,
    #[serde(default)]
    pub grader_failures: Vec<GraderFailure>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactIndex {
    pub schema_version: u32,
    pub run_id: String,
    pub suite_id: String,
    pub target_label: Option<String>,
    pub files: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SuiteRunReport {
    pub manifest: RunManifest,
    pub suite: SuiteSummary,
    pub trials: Vec<TrialRecord>,
}

fn target_path_key(target: &ExecutionTarget) -> String {
    format!("{}@{}", target.provider, target.model)
}

pub(crate) struct IncrementalSuiteWriter {
    root: PathBuf,
    suite_dir: PathBuf,
    manifest_path: PathBuf,
    summary_path: PathBuf,
    files: Vec<String>,
    run_id: String,
    suite_id: String,
    target_label: String,
    manifest: RunManifest,
}

impl IncrementalSuiteWriter {
    pub(crate) fn new(
        root: impl AsRef<Path>,
        suite_id: &str,
        target: &ExecutionTarget,
        manifest: &RunManifest,
    ) -> EvalResult<Self> {
        let root = root.as_ref().to_path_buf();
        let suite_dir = root.join(&manifest.run_id).join(suite_id).to_path_buf();
        let target_dir = suite_dir.join(target_path_key(target));
        fs::create_dir_all(&target_dir)?;

        let manifest_path = target_dir.join("manifest.json");
        let summary_path = target_dir.join("suite-summary.json");

        let mut writer = Self {
            root,
            suite_dir,
            manifest_path,
            summary_path,
            files: Vec::new(),
            run_id: manifest.run_id.clone(),
            suite_id: suite_id.to_string(),
            target_label: target.display_label(),
            manifest: manifest.clone(),
        };
        writer
            .files
            .push(relative_file(&writer.root, &writer.manifest_path));
        writer.write_manifest()?;
        Ok(writer)
    }

    pub(crate) fn write_trial(&mut self, trial: &TrialRecord) -> EvalResult<()> {
        let trial_dir = self
            .suite_dir
            .join(&trial.eval_id)
            .join(target_path_key(&trial.target));
        fs::create_dir_all(&trial_dir)?;
        let trial_path = trial_dir.join(format!(
            "trial-{:03}__{}__{}.json",
            trial.trial_index + 1,
            trial.eval_id,
            trial.trial_id
        ));
        write_json(
            &trial_path,
            &PersistedTrialRecord::from_trial_record(trial)?,
        )?;
        self.files.push(relative_file(&self.root, &trial_path));
        self.write_manifest()
    }

    pub(crate) fn finish(&mut self, report: &SuiteRunReport) -> EvalResult<ArtifactIndex> {
        write_json(&self.summary_path, &report.suite)?;
        self.files
            .push(relative_file(&self.root, &self.summary_path));
        self.write_manifest()?;
        Ok(self.current_index())
    }

    fn write_manifest(&mut self) -> EvalResult<()> {
        self.manifest.files = self.files.clone();
        write_json(&self.manifest_path, &self.manifest)
    }

    fn current_index(&self) -> ArtifactIndex {
        ArtifactIndex {
            schema_version: SCHEMA_VERSION,
            run_id: self.run_id.clone(),
            suite_id: self.suite_id.clone(),
            target_label: Some(self.target_label.clone()),
            files: self.files.clone(),
        }
    }
}

impl SuiteRunReport {
    pub fn summary_markdown(&self) -> String {
        format!(
            "# {} ({})\n\n- total trials: {}\n- pass rate: {:.0}%\n- mean score: {:.2}\n- mean duration: {:.0} ms\n",
            self.suite.suite_id,
            self.suite.target.display_label(),
            self.suite.total_trials,
            self.suite.pass_rate * 100.0,
            self.suite.mean_score,
            self.suite.mean_duration.as_millis()
        )
    }

    pub fn write_to(&self, root: impl AsRef<Path>) -> EvalResult<ArtifactIndex> {
        let root = root.as_ref();
        let suite_dir = root
            .join(&self.manifest.run_id)
            .join(&self.suite.suite_id)
            .to_path_buf();
        let target_dir = suite_dir.join(target_path_key(&self.suite.target));
        fs::create_dir_all(&target_dir)?;

        let manifest_path = target_dir.join("manifest.json");
        let summary_path = target_dir.join("suite-summary.json");
        write_json(&summary_path, &self.suite)?;

        let mut files = vec![
            relative_file(root, &manifest_path),
            relative_file(root, &summary_path),
        ];

        for trial in &self.trials {
            let trial_dir = suite_dir
                .join(&trial.eval_id)
                .join(target_path_key(&self.suite.target));
            fs::create_dir_all(&trial_dir)?;
            let trial_path = trial_dir.join(format!(
                "trial-{:03}__{}__{}.json",
                trial.trial_index + 1,
                trial.eval_id,
                trial.trial_id
            ));
            write_json(
                &trial_path,
                &PersistedTrialRecord::from_trial_record(trial)?,
            )?;
            files.push(relative_file(root, &trial_path));
        }

        let index = ArtifactIndex {
            schema_version: SCHEMA_VERSION,
            run_id: self.manifest.run_id.clone(),
            suite_id: self.suite.suite_id.clone(),
            target_label: Some(self.suite.target.display_label()),
            files,
        };
        let mut manifest = self.manifest.clone();
        manifest.files = index.files.clone();
        write_json(&manifest_path, &manifest)?;
        Ok(index)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct PersistedTrialRecord {
    pub trial_id: String,
    pub suite_id: String,
    pub eval_id: String,
    pub target: PersistedTrialTarget,
    pub timing: PersistedTrialTiming,
    pub grading: PersistedTrialGrading,
    pub transcript: Vec<PersistedTranscriptEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct PersistedTrialTarget {
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct PersistedTrialTiming {
    pub started_at_ms: u128,
    pub finished_at_ms: u128,
    pub duration_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct PersistedTrialGrading {
    pub passed: bool,
    pub mean_score: f32,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PersistedTranscriptEvent {
    System {
        value: String,
    },
    User {
        value: serde_json::Value,
        step: Option<usize>,
    },
    Assistant {
        value: serde_json::Value,
    },
    Thinking {
        value: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        id: String,
        name: String,
        result: serde_json::Value,
    },
    Grade {
        name: String,
        score: Option<f32>,
        summary: Option<String>,
        evidence: Option<serde_json::Value>,
        failed: bool,
        error: Option<String>,
        step: Option<usize>,
    },
    Error {
        reason: String,
    },
}

impl PersistedTrialRecord {
    fn from_trial_record(trial: &TrialRecord) -> EvalResult<Self> {
        let parsed_trial = trial
            .trial
            .clone()
            .map(serde_json::from_value::<AgentTrial<serde_json::Value>>)
            .transpose()?;
        let agent_trial = parsed_trial.unwrap_or_else(|| AgentTrial {
            transcript: Vec::new(),
            final_reply: None,
            tool_trace: Vec::new(),
            grades: BTreeMap::new(),
            grader_failures: Vec::new(),
            metadata: serde_json::Value::Null,
        });

        Ok(Self {
            transcript: persisted_transcript(agent_trial.transcript),
            trial_id: trial.trial_id.clone(),
            suite_id: trial.suite_id.clone(),
            eval_id: trial.eval_id.clone(),
            target: PersistedTrialTarget {
                label: trial.target.display_label(),
            },
            timing: PersistedTrialTiming {
                started_at_ms: trial.started_at.as_millis(),
                finished_at_ms: trial.finished_at.as_millis(),
                duration_ms: trial.duration.as_millis(),
            },
            grading: PersistedTrialGrading {
                passed: trial.passed,
                mean_score: trial.mean_score,
                error: trial.error.clone(),
            },
        })
    }
}

fn persisted_transcript(events: Vec<RecordedEvent>) -> Vec<PersistedTranscriptEvent> {
    let mut transcript = Vec::new();
    let mut current_step = None;

    for event in events {
        match event {
            RecordedEvent::StepStarted { step_index, input } => {
                current_step = Some(step_index);
                transcript.push(PersistedTranscriptEvent::User {
                    value: input,
                    step: Some(step_index),
                });
            }
            RecordedEvent::Message { role, content } => match role {
                RecordedMessageRole::System => {
                    transcript.push(PersistedTranscriptEvent::System { value: content })
                }
                RecordedMessageRole::User => {
                    if current_step.is_none() {
                        transcript.push(PersistedTranscriptEvent::User {
                            value: serde_json::Value::String(content),
                            step: None,
                        });
                    }
                }
                RecordedMessageRole::Assistant => {
                    transcript.push(PersistedTranscriptEvent::Assistant {
                        value: serde_json::Value::String(content),
                    })
                }
            },
            RecordedEvent::Thinking { content } => {
                transcript.push(PersistedTranscriptEvent::Thinking { value: content })
            }
            RecordedEvent::ToolCallRequested {
                id,
                name,
                arguments,
            } => transcript.push(PersistedTranscriptEvent::ToolCall {
                id,
                name,
                arguments,
            }),
            RecordedEvent::ToolExecutionCompleted { id, name, result } => {
                transcript.push(PersistedTranscriptEvent::ToolResult { id, name, result })
            }
            RecordedEvent::GraderCompleted {
                scope,
                grader,
                score,
                summary,
                evidence,
            } => transcript.push(PersistedTranscriptEvent::Grade {
                name: grader,
                score: Some(score),
                summary: Some(summary),
                evidence: Some(evidence),
                failed: false,
                error: None,
                step: grading_step(scope),
            }),
            RecordedEvent::GraderFailed {
                scope,
                grader,
                error,
            } => transcript.push(PersistedTranscriptEvent::Grade {
                name: grader,
                score: None,
                summary: None,
                evidence: None,
                failed: true,
                error: Some(error),
                step: grading_step(scope),
            }),
            RecordedEvent::Completed { reply } => {
                transcript.push(PersistedTranscriptEvent::Assistant { value: reply })
            }
            RecordedEvent::Error { reason } => {
                transcript.push(PersistedTranscriptEvent::Error { reason })
            }
            RecordedEvent::StepCompleted { .. } | RecordedEvent::GraderStarted { .. } => {}
        }
    }

    transcript
}

fn grading_step(scope: crate::trial::RecordedGradingScope) -> Option<usize> {
    match scope {
        crate::trial::RecordedGradingScope::Eval => None,
        crate::trial::RecordedGradingScope::TrajectoryStep { step_index } => Some(step_index),
    }
}

impl EvalRunReport {
    pub fn summary_markdown(&self) -> String {
        self.variants
            .iter()
            .map(SuiteRunReport::summary_markdown)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn summary_table(&self) -> String {
        let mut eval_ids = self
            .variants
            .iter()
            .flat_map(|variant| variant.suite.evals.iter().map(|eval| eval.eval_id.clone()))
            .collect::<Vec<_>>();
        eval_ids.sort();
        eval_ids.dedup();

        let mut sections = Vec::new();

        for eval_id in eval_ids {
            let eval_rows = self
                .variants
                .iter()
                .filter_map(|variant| {
                    variant
                        .suite
                        .evals
                        .iter()
                        .find(|eval| eval.eval_id == eval_id)
                        .map(|eval| (variant, eval))
                })
                .collect::<Vec<_>>();

            if eval_rows.is_empty() {
                continue;
            }

            let trial_count = eval_rows
                .iter()
                .map(|(_, eval)| eval.trial_count)
                .max()
                .unwrap_or(0);

            let mut ranked_rows = eval_rows
                .iter()
                .map(|(variant, eval)| {
                    (
                        variant.suite.target.display_label(),
                        eval.mean_score,
                        eval.mean_duration,
                        eval.grader_means.clone(),
                    )
                })
                .collect::<Vec<_>>();
            ranked_rows.sort_by(|left, right| {
                right
                    .1
                    .partial_cmp(&left.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(left.0.cmp(&right.0))
            });

            sections.push(format!("== Eval: {eval_id} (~{trial_count} trials) =="));
            sections.push(String::new());
            let eval_mean_duration =
                mean_duration(eval_rows.iter().map(|(_, eval)| eval.mean_duration));
            sections.push(format!(
                "avg duration ⏱  {} ms",
                eval_mean_duration.as_millis()
            ));
            sections.push(String::new());
            sections.push("final 🏁".to_string());
            let provider_width = ranked_rows
                .iter()
                .map(|(provider, _, _, _)| provider.len())
                .max()
                .unwrap_or(0);
            let duration_width = ranked_rows
                .iter()
                .map(|(_, _, duration, _)| format!("{} ms", duration.as_millis()).len())
                .max()
                .unwrap_or(0);
            let mut current_rank = 1usize;
            let mut previous_score: Option<f32> = None;
            for (index, (provider, score, duration, _)) in ranked_rows.iter().enumerate() {
                if let Some(previous) = previous_score
                    && (previous - score).abs() >= f32::EPSILON
                {
                    current_rank = index + 1;
                }
                let duration = format!("{} ms", duration.as_millis());
                sections.push(format!(
                    "  {provider:<provider_width$}  {score:.2}  {duration:>duration_width$}  {}",
                    medal_for_rank(current_rank),
                    provider_width = provider_width,
                    duration_width = duration_width,
                ));
                previous_score = Some(*score);
            }

            sections.push(String::new());
            sections.push("grades 🔎".to_string());
            for (provider, _, _, graders) in &ranked_rows {
                sections.push(format!("  {provider}"));
                if graders.is_empty() {
                    sections.push("    overall  0.00".to_string());
                    sections.push(String::new());
                    continue;
                }

                let grade_width = graders
                    .iter()
                    .map(|grader| grader.name.len())
                    .max()
                    .unwrap_or(0);
                for grader in graders {
                    sections.push(format!(
                        "    {grade:<grade_width$}  {score:.2}",
                        grade = grader.name,
                        score = grader.mean_score,
                        grade_width = grade_width,
                    ));
                }
                sections.push(String::new());
            }
        }

        while sections.last().is_some_and(|line| line.is_empty()) {
            sections.pop();
        }
        sections.join("\n")
    }

    pub fn write_to(&self, root: impl AsRef<Path>) -> EvalResult<ArtifactIndex> {
        let root = root.as_ref();
        let manifest_dir = root.join(&self.manifest.run_id);
        fs::create_dir_all(&manifest_dir)?;

        let manifest_path = manifest_dir.join("manifest.json");
        let mut files = vec![relative_file(root, &manifest_path)];
        for variant in &self.variants {
            let index = variant.write_to(root)?;
            files.extend(index.files);
        }

        let index = ArtifactIndex {
            schema_version: SCHEMA_VERSION,
            run_id: self.manifest.run_id.clone(),
            suite_id: self
                .manifest
                .suites
                .first()
                .cloned()
                .unwrap_or_else(|| "unknown-suite".to_string()),
            target_label: None,
            files,
        };
        let mut manifest = self.manifest.clone();
        manifest.files = index.files.clone();
        write_json(&manifest_path, &manifest)?;
        Ok(index)
    }
}

pub(crate) fn now_since_epoch() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

pub(crate) fn run_id() -> String {
    let now = chrono::Utc::now();
    format!(
        "run-{}",
        now.to_rfc3339_opts(SecondsFormat::Secs, true)
            .replace(':', "-")
    )
}

pub(crate) fn build_summary<State, A>(
    suite: &Suite<State, A>,
    run_id: &str,
    target: &ExecutionTarget,
    trials: &[TrialRecord],
) -> SuiteSummary
where
    State: Send + Sync + 'static,
    A: Agent,
{
    let total_trials = trials.len();
    let passed_trials = trials.iter().filter(|trial| trial.passed).count();
    let mean_score = mean(trials.iter().map(|trial| trial.mean_score));
    let mean_duration = mean_duration(trials.iter().map(|trial| trial.duration));

    let evals = suite
        .evals()
        .iter()
        .map(|eval| build_eval_aggregate(eval, trials))
        .collect();

    SuiteSummary {
        schema_version: SCHEMA_VERSION,
        run_id: run_id.to_string(),
        suite_id: suite.id().to_string(),
        target: target.clone(),
        total_evals: suite.evals().len(),
        total_trials,
        passed_trials,
        pass_rate: ratio(passed_trials, total_trials),
        mean_score,
        mean_duration,
        evals,
    }
}

fn build_eval_aggregate<State, A>(eval: &Eval<State, A>, trials: &[TrialRecord]) -> EvalAggregate
where
    State: Send + Sync + 'static,
    A: Agent,
{
    let eval_trials: Vec<&TrialRecord> = trials
        .iter()
        .filter(|trial| trial.eval_id == eval.id())
        .collect();
    let passed_trials = eval_trials.iter().filter(|trial| trial.passed).count();

    let grader_names = eval
        .graders()
        .iter()
        .map(|grader| grader.name().to_string())
        .chain(
            eval_trials
                .iter()
                .flat_map(|trial| trial.grades.keys().cloned()),
        )
        .collect::<BTreeSet<_>>();

    let grader_means = grader_names
        .into_iter()
        .map(|grader_name| {
            let grader_name_for_score = grader_name.clone();
            let grader_name_for_pass = grader_name.clone();
            let scores = eval_trials.iter().map(|trial| {
                trial
                    .grades
                    .get(&grader_name_for_score)
                    .map(|grade| grade.score)
                    .unwrap_or(0.0)
            });
            let passed_trials_for_grader = eval_trials
                .iter()
                .filter(|trial| {
                    trial
                        .grades
                        .get(&grader_name_for_pass)
                        .map(|grade| is_passing_score(grade.score))
                        .unwrap_or(false)
                })
                .count();

            GraderAggregate {
                name: grader_name,
                mean_score: mean(scores),
                pass_rate: ratio(passed_trials_for_grader, eval_trials.len()),
            }
        })
        .collect();

    EvalAggregate {
        eval_id: eval.id().to_string(),
        trial_count: eval_trials.len(),
        passed_trials,
        pass_rate: ratio(passed_trials, eval_trials.len()),
        mean_score: mean(eval_trials.iter().map(|trial| trial.mean_score)),
        mean_duration: mean_duration(eval_trials.iter().map(|trial| trial.duration)),
        grader_means,
    }
}

fn mean(values: impl IntoIterator<Item = f32>) -> f32 {
    let mut count = 0usize;
    let mut total = 0.0f32;
    for value in values {
        count += 1;
        total += value;
    }
    if count == 0 {
        return 0.0;
    }
    total / count as f32
}

fn mean_duration(values: impl IntoIterator<Item = Duration>) -> Duration {
    let mut count = 0usize;
    let mut total = Duration::ZERO;
    for value in values {
        count += 1;
        total += value;
    }
    if count == 0 {
        return Duration::ZERO;
    }
    total / count as u32
}

fn ratio(numerator: usize, denominator: usize) -> f32 {
    if denominator == 0 {
        return 0.0;
    }
    numerator as f32 / denominator as f32
}

fn write_json(path: &Path, value: &impl Serialize) -> EvalResult<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes)?;
    Ok(())
}

fn relative_file(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn medal_for_rank(rank: usize) -> &'static str {
    match rank {
        1 => "🥇",
        2 => "🥈",
        3 => "🥉",
        _ => "",
    }
}
