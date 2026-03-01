use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use borg_core::ExecutionResult;
use tracing::{debug, info};

use crate::types::ShellModeContext;

const DEFAULT_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone)]
pub struct ShellModeRuntime {
    default_timeout: Duration,
    default_working_directory: Option<PathBuf>,
}

impl Default for ShellModeRuntime {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            default_working_directory: None,
        }
    }
}

impl ShellModeRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_default_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = timeout;
        self
    }

    pub fn with_working_directory(mut self, dir: PathBuf) -> Self {
        self.default_working_directory = Some(dir);
        self
    }

    pub fn execute(&self, command: &str, context: ShellModeContext) -> Result<ExecutionResult> {
        let timeout = context.timeout(self.default_timeout);

        let cwd = context
            .working_directory()
            .cloned()
            .or_else(|| self.default_working_directory.clone());

        info!(target: "borg_shellmode", command = %command, cwd = ?cwd, timeout_secs = timeout.as_secs(), "executing shell command");

        let start = Instant::now();

        let mut cmd = Command::new("sh");
        cmd.args(["-c", command]);

        if let Some(ref dir) = cwd {
            cmd.current_dir(dir);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd
            .output()
            .with_context(|| format!("failed to execute command: {}", command))?;

        let duration = start.elapsed();
        let exit_code = output.status.code().unwrap_or(-1);

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        debug!(target: "borg_shellmode", exit_code = exit_code, stdout_len = stdout.len(), stderr_len = stderr.len(), "shell command completed");

        Ok(ExecutionResult {
            stdout: stdout.clone(),
            stderr: stderr.clone(),
            result_json: serde_json::json!({
                "exit_code": exit_code,
                "stdout": stdout,
                "stderr": stderr,
                "duration": {
                    "secs": duration.as_secs(),
                    "nanos": duration.subsec_nanos()
                }
            }),
            duration,
        })
    }
}
