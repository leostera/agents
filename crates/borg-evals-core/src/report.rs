use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::case::Case;
use crate::config::ExecutionTarget;
use crate::error::EvalResult;
use crate::grade::GradeResult;
use crate::suite::Suite;
use crate::trial::AgentTrial;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunManifest {
    pub schema_version: u32,
    pub run_id: String,
    pub started_at_ms: u128,
    pub finished_at_ms: u128,
    pub suites: Vec<String>,
    pub targets: Vec<ExecutionTarget>,
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
    pub total_cases: usize,
    pub total_trials: usize,
    pub passed_trials: usize,
    pub pass_rate: f32,
    pub mean_score: f32,
    pub cases: Vec<CaseAggregate>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CaseAggregate {
    pub case_id: String,
    pub trial_count: usize,
    pub passed_trials: usize,
    pub pass_rate: f32,
    pub mean_score: f32,
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
    pub run_id: String,
    pub suite_id: String,
    pub target: ExecutionTarget,
    pub case_id: String,
    pub trial_index: usize,
    pub passed: bool,
    pub mean_score: f32,
    pub trial: Option<AgentTrial>,
    pub error: Option<String>,
    pub grades: Vec<GradeResult>,
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

impl SuiteRunReport {
    pub fn summary_markdown(&self) -> String {
        format!(
            "# {} ({})\n\n- total trials: {}\n- pass rate: {:.0}%\n- mean score: {:.2}\n",
            self.suite.suite_id,
            self.suite.target.label,
            self.suite.total_trials,
            self.suite.pass_rate * 100.0,
            self.suite.mean_score
        )
    }

    pub fn write_to(&self, root: impl AsRef<Path>) -> EvalResult<ArtifactIndex> {
        let root = root.as_ref();
        let suite_dir = root
            .join("results")
            .join(&self.suite.suite_id)
            .join(&self.manifest.run_id)
            .join(&self.suite.target.label);
        fs::create_dir_all(&suite_dir)?;

        let manifest_path = suite_dir.join("manifest.json");
        let summary_path = suite_dir.join("suite-summary.json");
        let markdown_path = suite_dir.join("suite-summary.md");

        write_json(&manifest_path, &self.manifest)?;
        write_json(&summary_path, &self.suite)?;
        fs::write(&markdown_path, self.summary_markdown())?;

        let mut files = vec![
            relative_file(root, &manifest_path),
            relative_file(root, &summary_path),
            relative_file(root, &markdown_path),
        ];

        for trial in &self.trials {
            let trial_path = suite_dir.join(format!(
                "trial-{:03}__{}.json",
                trial.trial_index + 1,
                trial.case_id
            ));
            write_json(&trial_path, trial)?;
            files.push(relative_file(root, &trial_path));
        }

        let index = ArtifactIndex {
            schema_version: SCHEMA_VERSION,
            run_id: self.manifest.run_id.clone(),
            suite_id: self.suite.suite_id.clone(),
            target_label: Some(self.suite.target.label.clone()),
            files,
        };

        write_json(&suite_dir.join("artifact-index.json"), &index)?;
        Ok(index)
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

    pub fn write_to(&self, root: impl AsRef<Path>) -> EvalResult<ArtifactIndex> {
        let root = root.as_ref();
        let manifest_dir = root
            .join("results")
            .join(
                self.manifest
                    .suites
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "unknown-suite".to_string()),
            )
            .join(&self.manifest.run_id);
        fs::create_dir_all(&manifest_dir)?;

        let manifest_path = manifest_dir.join("manifest.json");
        write_json(&manifest_path, &self.manifest)?;

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

        write_json(&manifest_dir.join("artifact-index.json"), &index)?;
        Ok(index)
    }
}

pub(crate) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub(crate) fn run_id() -> String {
    format!("run-{}", now_ms())
}

pub(crate) fn build_summary(
    suite: &Suite,
    run_id: &str,
    target: &ExecutionTarget,
    trials: &[TrialRecord],
) -> SuiteSummary {
    let total_trials = trials.len();
    let passed_trials = trials.iter().filter(|trial| trial.passed).count();
    let mean_score = mean(trials.iter().map(|trial| trial.mean_score));

    let cases = suite
        .cases()
        .iter()
        .map(|case| build_case_aggregate(case, trials))
        .collect();

    SuiteSummary {
        schema_version: SCHEMA_VERSION,
        run_id: run_id.to_string(),
        suite_id: suite.id().to_string(),
        target: target.clone(),
        total_cases: suite.cases().len(),
        total_trials,
        passed_trials,
        pass_rate: ratio(passed_trials, total_trials),
        mean_score,
        cases,
    }
}

fn build_case_aggregate(case: &Case, trials: &[TrialRecord]) -> CaseAggregate {
    let case_trials: Vec<&TrialRecord> = trials
        .iter()
        .filter(|trial| trial.case_id == case.id())
        .collect();
    let passed_trials = case_trials.iter().filter(|trial| trial.passed).count();

    let grader_means = case
        .graders()
        .iter()
        .map(|grader| {
            let grades: Vec<&GradeResult> = case_trials
                .iter()
                .flat_map(|trial| trial.grades.iter())
                .filter(|grade| grade.name == grader.name())
                .collect();

            GraderAggregate {
                name: grader.name().to_string(),
                mean_score: mean(grades.iter().map(|grade| grade.score)),
                pass_rate: ratio(grades.iter().filter(|grade| grade.passed).count(), grades.len()),
            }
        })
        .collect();

    CaseAggregate {
        case_id: case.id().to_string(),
        trial_count: case_trials.len(),
        passed_trials,
        pass_rate: ratio(passed_trials, case_trials.len()),
        mean_score: mean(case_trials.iter().map(|trial| trial.mean_score)),
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
