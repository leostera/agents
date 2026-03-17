use std::collections::BTreeMap;
use std::io::{Stdout, Write};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const RED: &str = "\x1b[31m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";
const LABEL_COL_WIDTH: usize = 72;
const MARKS_COL_WIDTH: usize = 16;
const SCORE_COL_WIDTH: usize = 10;
const TIME_COL_WIDTH: usize = 10;
pub const HEADER_LABEL: &str = "Eval";
pub const HEADER_MARKS: &str = "Trials";
pub const HEADER_SCORE: &str = "Score";
pub const HEADER_TIME: &str = "Time";

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunEvent {
    RunStarted {
        suite_count: usize,
        targets: Vec<String>,
        trials: usize,
        output_dir: String,
    },
    SuiteStarted {
        suite_id: String,
        target_label: String,
        eval_count: usize,
        trial_count: usize,
    },
    EvalStarted {
        suite_id: String,
        eval_id: String,
        target_label: String,
        trials: usize,
    },
    TrialFinished {
        suite_id: String,
        eval_id: String,
        trial_id: String,
        trial_index: usize,
        target_label: String,
        passed: bool,
        mean_score: f32,
        duration_ms: u128,
        error: Option<String>,
    },
    EvalFinished {
        suite_id: String,
        eval_id: String,
        target_label: String,
        trial_count: usize,
        passed_trials: usize,
        mean_score: f32,
        mean_duration_ms: u128,
    },
    SuiteFinished {
        suite_id: String,
        target_label: String,
        total_trials: usize,
        passed_trials: usize,
        mean_score: f32,
        mean_duration_ms: u128,
    },
    RunFinished {
        suite_count: usize,
        variant_count: usize,
    },
}

pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: RunEvent);
}

#[derive(Clone, Default)]
pub struct NoopEventSink;

impl EventSink for NoopEventSink {
    fn emit(&self, _event: RunEvent) {}
}

pub type SharedEventSink = Arc<dyn EventSink>;

pub struct JsonEventSink {
    writer: Mutex<Stdout>,
}

impl JsonEventSink {
    pub fn stdout() -> Self {
        Self {
            writer: Mutex::new(std::io::stdout()),
        }
    }
}

impl EventSink for JsonEventSink {
    fn emit(&self, event: RunEvent) {
        let mut writer = self.writer.lock().expect("json event sink lock poisoned");
        serde_json::to_writer(&mut *writer, &event).expect("serialize run event");
        writeln!(&mut *writer).expect("write run event newline");
        writer.flush().expect("flush run event output");
    }
}

pub struct ProgressEventSink {
    state: Mutex<ProgressState>,
}

struct ProgressState {
    multi: MultiProgress,
    rows: BTreeMap<String, ProgressRow>,
    header_printed: bool,
}

struct ProgressRow {
    bar: ProgressBar,
    suite_id: String,
    eval_id: String,
    target_label: String,
    trials: usize,
    marks: Vec<char>,
    total_score: f32,
    total_duration_ms: u128,
    mean_score: Option<f32>,
    mean_duration_ms: Option<u128>,
}

impl ProgressEventSink {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(ProgressState {
                multi: MultiProgress::new(),
                rows: BTreeMap::new(),
                header_printed: false,
            }),
        }
    }

    pub fn header_line() -> String {
        format!(
            "{:<LABEL_COL_WIDTH$} {:<MARKS_COL_WIDTH$} {:>SCORE_COL_WIDTH$} {:>TIME_COL_WIDTH$}",
            format!("{BOLD}{HEADER_LABEL}{RESET}"),
            format!("{BOLD}{HEADER_MARKS}{RESET}"),
            format!("{BOLD}{HEADER_SCORE}{RESET}"),
            format!("{BOLD}{HEADER_TIME}{RESET}"),
        )
    }

    fn key(suite_id: &str, eval_id: &str, target_label: &str) -> String {
        format!("{suite_id}/{target_label}/{eval_id}")
    }

    fn row_prefix(suite_id: &str, eval_id: &str, target_label: &str) -> String {
        format!("{suite_id} :: {target_label} :: {eval_id}")
    }

    fn fit_label(label: &str) -> String {
        let char_count = label.chars().count();
        if char_count <= LABEL_COL_WIDTH {
            return format!("{label:<LABEL_COL_WIDTH$}");
        }

        let truncated: String = label.chars().take(LABEL_COL_WIDTH - 1).collect();
        format!("{truncated}…")
    }

    fn row_marks(row: &ProgressRow) -> String {
        row.marks
            .iter()
            .map(|mark| match mark {
                'F' => format!("{RED}F{RESET}"),
                other => other.to_string(),
            })
            .collect::<String>()
    }

    fn row_message(row: &ProgressRow) -> String {
        let label = Self::fit_label(&Self::row_prefix(
            &row.suite_id,
            &row.eval_id,
            &row.target_label,
        ));
        let score = row
            .mean_score
            .map(|score| format!("{score:.2}"))
            .unwrap_or_default();
        let time_ms = row
            .mean_duration_ms
            .map(|duration| format!("{duration}ms"))
            .unwrap_or_default();
        format!(
            "{} {:<MARKS_COL_WIDTH$} {:>SCORE_COL_WIDTH$} {:>TIME_COL_WIDTH$}",
            label,
            Self::row_marks(row),
            score,
            time_ms,
        )
    }

    fn make_bar(multi: &MultiProgress, trials: usize, message: String) -> ProgressBar {
        let bar = multi.add(ProgressBar::new(trials as u64));
        bar.set_style(
            ProgressStyle::with_template("{msg}").expect("valid indicatif progress template"),
        );
        bar.set_message(message);
        bar
    }
}

impl Default for ProgressEventSink {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSink for ProgressEventSink {
    fn emit(&self, event: RunEvent) {
        let mut state = self
            .state
            .lock()
            .expect("progress event sink lock poisoned");
        if !state.header_printed {
            state.header_printed = true;
        }
        match event {
            RunEvent::EvalStarted {
                suite_id,
                eval_id,
                target_label,
                trials,
            } => {
                let key = Self::key(&suite_id, &eval_id, &target_label);
                if !state.rows.contains_key(&key) {
                    let bar = Self::make_bar(
                        &state.multi,
                        trials,
                        Self::row_prefix(&suite_id, &eval_id, &target_label),
                    );
                    state.rows.insert(
                        key,
                        ProgressRow {
                            bar,
                            suite_id,
                            eval_id,
                            target_label,
                            trials,
                            marks: Vec::new(),
                            total_score: 0.0,
                            total_duration_ms: 0,
                            mean_score: None,
                            mean_duration_ms: None,
                        },
                    );
                }
            }
            RunEvent::TrialFinished {
                suite_id,
                eval_id,
                target_label,
                passed,
                mean_score,
                duration_ms,
                ..
            } => {
                let key = Self::key(&suite_id, &eval_id, &target_label);
                if !state.rows.contains_key(&key) {
                    let bar = Self::make_bar(
                        &state.multi,
                        0,
                        Self::row_prefix(&suite_id, &eval_id, &target_label),
                    );
                    state.rows.insert(
                        key.clone(),
                        ProgressRow {
                            bar,
                            suite_id,
                            eval_id,
                            target_label,
                            trials: 0,
                            marks: Vec::new(),
                            total_score: 0.0,
                            total_duration_ms: 0,
                            mean_score: None,
                            mean_duration_ms: None,
                        },
                    );
                }
                let row = state.rows.get_mut(&key).expect("progress row inserted");
                row.marks.push(if passed { '.' } else { 'F' });
                row.total_score += mean_score;
                row.total_duration_ms += duration_ms;
                let completed_trials = row.marks.len() as u128;
                row.mean_score = Some(row.total_score / completed_trials as f32);
                row.mean_duration_ms = Some(row.total_duration_ms / completed_trials);
                row.bar.set_length(row.trials as u64);
                row.bar.set_position(row.marks.len() as u64);
                row.bar.set_message(Self::row_message(row));
            }
            RunEvent::EvalFinished {
                suite_id,
                eval_id,
                target_label,
                trial_count,
                mean_score,
                mean_duration_ms,
                ..
            } => {
                let key = Self::key(&suite_id, &eval_id, &target_label);
                if !state.rows.contains_key(&key) {
                    let bar = Self::make_bar(
                        &state.multi,
                        trial_count,
                        Self::row_prefix(&suite_id, &eval_id, &target_label),
                    );
                    state.rows.insert(
                        key.clone(),
                        ProgressRow {
                            bar,
                            suite_id,
                            eval_id,
                            target_label,
                            trials: trial_count,
                            marks: Vec::new(),
                            total_score: 0.0,
                            total_duration_ms: 0,
                            mean_score: None,
                            mean_duration_ms: None,
                        },
                    );
                }
                let row = state.rows.get_mut(&key).expect("progress row inserted");
                row.trials = trial_count;
                row.mean_score = Some(mean_score);
                row.mean_duration_ms = Some(mean_duration_ms);
                row.bar.set_length(trial_count as u64);
                row.bar.set_position(row.marks.len() as u64);
                row.bar.finish_with_message(Self::row_message(row));
            }
            _ => {}
        }
    }
}

fn sink_cell() -> &'static RwLock<SharedEventSink> {
    static GLOBAL_SINK: OnceLock<RwLock<SharedEventSink>> = OnceLock::new();
    GLOBAL_SINK.get_or_init(|| RwLock::new(Arc::new(NoopEventSink)))
}

pub fn set_global_sink(sink: SharedEventSink) {
    let mut guard = sink_cell()
        .write()
        .expect("global eval event sink lock poisoned");
    *guard = sink;
}

pub fn global_sink() -> SharedEventSink {
    sink_cell()
        .read()
        .expect("global eval event sink lock poisoned")
        .clone()
}

pub fn emit(event: RunEvent) {
    global_sink().emit(event);
}
