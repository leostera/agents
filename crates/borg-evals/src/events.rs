use std::collections::BTreeMap;
use std::io::{IsTerminal, Stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread;
use std::time::Duration;

use ratatui::backend::CrosstermBackend;
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::{Terminal, TerminalOptions, Viewport};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

type CrosstermTerminal = Terminal<CrosstermBackend<Stdout>>;
const MIN_INLINE_VIEWPORT_HEIGHT: u16 = 4;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunEvent {
    RunStarted {
        suite_count: usize,
        targets: Vec<String>,
        trials: usize,
        output_dir: String,
    },
    RunPlanned {
        suites: Vec<PlannedSuiteRun>,
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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PlannedSuiteRun {
    pub crate_name: String,
    pub suite_id: String,
    pub target_labels: Vec<String>,
    pub eval_ids: Vec<String>,
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
    state: Arc<Mutex<ProgressState>>,
    running: Arc<AtomicBool>,
}

struct ProgressState {
    rows: BTreeMap<String, ProgressRow>,
    terminal: Option<CrosstermTerminal>,
    terminal_ready: bool,
    viewport_height: u16,
}

struct ProgressRow {
    suite_id: String,
    eval_id: String,
    target_label: String,
    trials: usize,
    status: ProgressStatus,
    marks: Vec<char>,
    total_score: f32,
    total_duration_ms: u128,
    mean_score: Option<f32>,
    mean_duration_ms: Option<u128>,
}

enum ProgressStatus {
    Pending,
    Running { frame: usize },
    Finished { outcome: ProgressOutcome },
}

#[derive(Clone, Copy)]
enum ProgressOutcome {
    Passed,
    Failed,
    Errored,
}

impl ProgressEventSink {
    const TRIALS_COLUMN_WIDTH: u16 = 32;
    const SCORE_COLUMN_WIDTH: u16 = 8;
    const TIME_COLUMN_WIDTH: u16 = 10;

    pub fn new() -> Self {
        let terminal = if std::io::stdout().is_terminal() {
            let backend = CrosstermBackend::new(std::io::stdout());
            let viewport_height = MIN_INLINE_VIEWPORT_HEIGHT;
            Terminal::with_options(
                backend,
                TerminalOptions {
                    viewport: Viewport::Inline(viewport_height),
                },
            )
            .ok()
        } else {
            None
        };
        let state = Arc::new(Mutex::new(ProgressState {
            rows: BTreeMap::new(),
            terminal,
            terminal_ready: std::io::stdout().is_terminal(),
            viewport_height: MIN_INLINE_VIEWPORT_HEIGHT,
        }));
        let running = Arc::new(AtomicBool::new(true));

        if std::io::stdout().is_terminal() {
            let ticker_state = state.clone();
            let ticker_running = running.clone();
            thread::spawn(move || {
                while ticker_running.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(100));
                    let Ok(mut state) = ticker_state.lock() else {
                        break;
                    };
                    if !state.terminal_ready {
                        break;
                    }
                    let mut advanced = false;
                    for row in state.rows.values_mut() {
                        if let ProgressStatus::Running { frame } = row.status {
                            row.status = ProgressStatus::Running { frame: frame + 1 };
                            advanced = true;
                        }
                    }
                    if advanced {
                        Self::render(&mut state);
                    }
                }
            });
        }

        Self { state, running }
    }

    fn key(suite_id: &str, eval_id: &str, target_label: &str) -> String {
        format!("{suite_id}/{target_label}/{eval_id}")
    }

    fn row_prefix(suite_id: &str, eval_id: &str, target_label: &str) -> String {
        format!("{suite_id} :: {target_label} :: {eval_id}")
    }

    fn truncate_label(label: &str, max_chars: usize) -> String {
        let len = label.chars().count();
        if len <= max_chars {
            return label.to_string();
        }
        if max_chars <= 1 {
            return "…".to_string();
        }

        let truncated: String = label.chars().take(max_chars - 1).collect();
        format!("{truncated}…")
    }

    fn row_label(row: &ProgressRow, max_chars: usize) -> Line<'static> {
        let mut spans = Vec::new();
        match row.status {
            ProgressStatus::Pending => {
                spans.push(Span::raw("  "));
            }
            ProgressStatus::Running { frame } => {
                const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                spans.push(Span::styled(
                    format!("{} ", SPINNER[frame % SPINNER.len()]),
                    Style::default().fg(Color::Cyan),
                ));
            }
            ProgressStatus::Finished { outcome } => {
                let (symbol, style) = match outcome {
                    ProgressOutcome::Passed => (
                        "✓ ",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    ProgressOutcome::Failed => ("✗ ", Style::default().fg(Color::Red)),
                    ProgressOutcome::Errored => (
                        "E ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                };
                spans.push(Span::styled(symbol, style));
            }
        }
        let label = Self::row_prefix(&row.suite_id, &row.eval_id, &row.target_label);
        spans.push(Span::raw(Self::truncate_label(&label, max_chars)));
        Line::from(spans)
    }

    fn row_marks_spans(row: &ProgressRow) -> Vec<Span<'static>> {
        row.marks
            .iter()
            .map(|mark| match mark {
                'F' => Span::styled("F", Style::default().fg(Color::Red)),
                'E' => Span::styled(
                    "E",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                other => Span::raw(other.to_string()),
            })
            .collect()
    }

    fn render(state: &mut ProgressState) {
        if !state.terminal_ready {
            return;
        }

        Self::ensure_viewport_height(state);

        let Some(terminal) = state.terminal.as_mut() else {
            return;
        };

        terminal
            .draw(|frame| {
                let fixed_width =
                    Self::TRIALS_COLUMN_WIDTH + Self::SCORE_COLUMN_WIDTH + Self::TIME_COLUMN_WIDTH;
                let label_width = frame
                    .area()
                    .width
                    .saturating_sub(fixed_width)
                    .saturating_sub(3);
                let label_chars = label_width.max(1) as usize;
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
                        Cell::from(Self::row_label(row, label_chars)),
                        Cell::from(Line::from(Self::row_marks_spans(row))),
                        Cell::from(score),
                        Cell::from(time_ms),
                    ])
                });
                let table = Table::new(
                    rows,
                    [
                        Constraint::Fill(1),
                        Constraint::Length(Self::TRIALS_COLUMN_WIDTH),
                        Constraint::Length(Self::SCORE_COLUMN_WIDTH),
                        Constraint::Length(Self::TIME_COLUMN_WIDTH),
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

    fn desired_viewport_height(row_count: usize) -> u16 {
        let desired = row_count
            .saturating_add(1)
            .max(MIN_INLINE_VIEWPORT_HEIGHT as usize);
        desired.min(u16::MAX as usize) as u16
    }

    fn ensure_viewport_height(state: &mut ProgressState) {
        let desired = Self::desired_viewport_height(state.rows.len());
        if desired == state.viewport_height {
            return;
        }

        let _old_terminal = state.terminal.take();
        let backend = CrosstermBackend::new(std::io::stdout());
        state.terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(desired),
            },
        )
        .ok();
        state.viewport_height = desired;
        state.terminal_ready = state.terminal.is_some();
    }

    fn finish(state: &mut ProgressState) {
        if !state.terminal_ready {
            return;
        }
        if let Some(terminal) = state.terminal.as_mut() {
            terminal.show_cursor().expect("show eval progress cursor");
        }
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
            RunEvent::RunPlanned { suites } => {
                for suite in suites {
                    for target_label in suite.target_labels {
                        for eval_id in &suite.eval_ids {
                            let key = Self::key(&suite.suite_id, eval_id, &target_label);
                            state.rows.entry(key).or_insert_with(|| ProgressRow {
                                suite_id: suite.suite_id.clone(),
                                eval_id: eval_id.clone(),
                                target_label: target_label.clone(),
                                trials: 0,
                                status: ProgressStatus::Pending,
                                marks: Vec::new(),
                                total_score: 0.0,
                                total_duration_ms: 0,
                                mean_score: None,
                                mean_duration_ms: None,
                            });
                        }
                    }
                }
            }
            RunEvent::EvalStarted {
                suite_id,
                eval_id,
                target_label,
                trials,
            } => {
                let key = Self::key(&suite_id, &eval_id, &target_label);
                state
                    .rows
                    .entry(key.clone())
                    .or_insert_with(|| ProgressRow {
                        suite_id,
                        eval_id,
                        target_label,
                        trials,
                        status: ProgressStatus::Running { frame: 0 },
                        marks: Vec::new(),
                        total_score: 0.0,
                        total_duration_ms: 0,
                        mean_score: None,
                        mean_duration_ms: None,
                    });
                let row = state.rows.get_mut(&key).expect("progress row inserted");
                row.trials = trials;
                row.status = ProgressStatus::Running { frame: 0 };
            }
            RunEvent::TrialFinished {
                suite_id,
                eval_id,
                target_label,
                passed,
                mean_score,
                duration_ms,
                error,
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
                            status: ProgressStatus::Pending,
                            marks: Vec::new(),
                            total_score: 0.0,
                            total_duration_ms: 0,
                            mean_score: None,
                            mean_duration_ms: None,
                        },
                    );
                }
                let row = state.rows.get_mut(&key).expect("progress row inserted");
                row.status = match row.status {
                    ProgressStatus::Running { frame } => {
                        ProgressStatus::Running { frame: frame + 1 }
                    }
                    _ => ProgressStatus::Running { frame: 0 },
                };
                row.marks.push(if error.is_some() {
                    'E'
                } else if passed {
                    '.'
                } else {
                    'F'
                });
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
                            status: ProgressStatus::Pending,
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
                row.status = ProgressStatus::Finished {
                    outcome: if row.marks.contains(&'E') {
                        ProgressOutcome::Errored
                    } else if mean_score > 0.8 {
                        ProgressOutcome::Passed
                    } else {
                        ProgressOutcome::Failed
                    },
                };
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
        self.running.store(false, Ordering::Relaxed);
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
