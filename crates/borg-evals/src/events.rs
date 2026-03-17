use std::collections::BTreeMap;
use std::io::{Stdout, Write};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use ratatui::backend::CrosstermBackend;
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::{Terminal, TerminalOptions, Viewport};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

type CrosstermTerminal = Terminal<CrosstermBackend<Stdout>>;
const INLINE_VIEWPORT_HEIGHT: u16 = 12;

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
    rows: BTreeMap<String, ProgressRow>,
    terminal: CrosstermTerminal,
    terminal_ready: bool,
}

struct ProgressRow {
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
        let backend = CrosstermBackend::new(std::io::stdout());
        let terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(INLINE_VIEWPORT_HEIGHT),
            },
        )
        .expect("create eval progress terminal");
        Self {
            state: Mutex::new(ProgressState {
                rows: BTreeMap::new(),
                terminal,
                terminal_ready: true,
            }),
        }
    }

    fn key(suite_id: &str, eval_id: &str, target_label: &str) -> String {
        format!("{suite_id}/{target_label}/{eval_id}")
    }

    fn row_prefix(suite_id: &str, eval_id: &str, target_label: &str) -> String {
        format!("{suite_id} :: {target_label} :: {eval_id}")
    }

    fn row_marks_spans(row: &ProgressRow) -> Vec<Span<'static>> {
        row.marks
            .iter()
            .map(|mark| match mark {
                'F' => Span::styled("F", Style::default().fg(Color::Red)),
                other => Span::raw(other.to_string()),
            })
            .collect()
    }

    fn render(state: &mut ProgressState) {
        if !state.terminal_ready {
            return;
        }

        let rows = state.rows.values().map(|row| {
            let score = row
                .mean_score
                .map(|score| format!("{score:.2}"))
                .unwrap_or_default();
            let time_ms = row
                .mean_duration_ms
                .map(|duration| format!("{duration}ms"))
                .unwrap_or_default();
            Row::new(vec![
                Cell::from(Self::row_prefix(
                    &row.suite_id,
                    &row.eval_id,
                    &row.target_label,
                )),
                Cell::from(Line::from(Self::row_marks_spans(row))),
                Cell::from(score),
                Cell::from(time_ms),
            ])
        });

        state
            .terminal
            .draw(|frame| {
                let table = Table::new(
                    rows,
                    [
                        Constraint::Fill(1),
                        Constraint::Length(32),
                        Constraint::Length(8),
                        Constraint::Length(10),
                    ],
                )
                .header(
                    Row::new(vec!["Eval", "Trials", "Score", "Time"])
                        .style(Style::default().add_modifier(Modifier::BOLD)),
                )
                .block(Block::default().borders(Borders::NONE));

                frame.render_widget(table, frame.area());
            })
            .expect("draw eval progress table");
    }

    fn finish(state: &mut ProgressState) {
        if !state.terminal_ready {
            return;
        }
        state
            .terminal
            .show_cursor()
            .expect("show eval progress cursor");
        state.terminal_ready = false;
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
        match event {
            RunEvent::EvalStarted {
                suite_id,
                eval_id,
                target_label,
                trials,
            } => {
                let key = Self::key(&suite_id, &eval_id, &target_label);
                state.rows.entry(key).or_insert_with(|| ProgressRow {
                    suite_id,
                    eval_id,
                    target_label,
                    trials,
                    marks: Vec::new(),
                    total_score: 0.0,
                    total_duration_ms: 0,
                    mean_score: None,
                    mean_duration_ms: None,
                });
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
                    state.rows.insert(
                        key.clone(),
                        ProgressRow {
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
                    state.rows.insert(
                        key.clone(),
                        ProgressRow {
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
            }
            RunEvent::RunFinished { .. } => {
                Self::render(&mut state);
                Self::finish(&mut state);
                return;
            }
            _ => {}
        }
        Self::render(&mut state);
    }
}

impl Drop for ProgressEventSink {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            Self::finish(&mut state);
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
