use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use borg_core::ExecutionResult;
use serde_json::{Value, json};
use tracing::{debug, info, warn};

#[cfg(target_os = "macos")]
use mac_notification_sys::send_notification;

use crate::types::{MacOsExecutionData, MacOsPolicy};

const DEFAULT_TIMEOUT_SECONDS: u64 = 30;

#[derive(Debug, Clone)]
pub struct MacOsRuntime {
    policy: MacOsPolicy,
}

impl Default for MacOsRuntime {
    fn default() -> Self {
        Self {
            policy: MacOsPolicy {
                max_execution_seconds: DEFAULT_TIMEOUT_SECONDS,
                ..MacOsPolicy::default()
            },
        }
    }
}

impl MacOsRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_policy(mut self, policy: MacOsPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn policy(&self) -> &MacOsPolicy {
        &self.policy
    }

    pub fn list_shortcuts(
        &self,
        folder_name: Option<&str>,
        folders: bool,
        show_identifiers: bool,
        timeout_seconds: Option<u64>,
    ) -> Result<ExecutionResult<MacOsExecutionData>> {
        let mut args = vec!["list".to_string()];
        if folders {
            args.push("--folders".to_string());
        }
        if show_identifiers {
            args.push("--show-identifiers".to_string());
        }
        if let Some(folder_name) = folder_name.map(str::trim).filter(|value| !value.is_empty()) {
            args.push("--folder-name".to_string());
            args.push(folder_name.to_string());
        }
        self.run_command_with_timeout("shortcuts", &args, timeout_seconds)
    }

    pub fn run_shortcut(
        &self,
        shortcut_name: &str,
        timeout_seconds: Option<u64>,
    ) -> Result<ExecutionResult<MacOsExecutionData>> {
        let shortcut_name = shortcut_name.trim();
        if shortcut_name.is_empty() {
            return Err(anyhow!("shortcut_name is required"));
        }
        if !self.policy.is_shortcut_allowed(shortcut_name) {
            return Err(anyhow!(
                "shortcut `{}` is not allowed by macOS policy",
                shortcut_name
            ));
        }
        let args = vec!["run".to_string(), shortcut_name.to_string()];
        self.run_command_with_timeout("shortcuts", &args, timeout_seconds)
    }

    pub fn run_applescript_template(
        &self,
        template_id: &str,
        parameters: &Value,
        timeout_seconds: Option<u64>,
    ) -> Result<ExecutionResult<MacOsExecutionData>> {
        let template_id = template_id.trim();
        if template_id.is_empty() {
            return Err(anyhow!("template_id is required"));
        }
        if !self.policy.is_template_allowed(template_id) {
            return Err(anyhow!(
                "script template `{}` is not allowed by macOS policy",
                template_id
            ));
        }
        let script = render_template_script(template_id, parameters)?;
        self.run_applescript_raw_internal(&script, timeout_seconds)
    }

    pub fn run_applescript_raw(
        &self,
        script: &str,
        timeout_seconds: Option<u64>,
    ) -> Result<ExecutionResult<MacOsExecutionData>> {
        if !self.policy.allow_raw_applescript {
            return Err(anyhow!(
                "raw AppleScript execution is disabled by policy (`allow_raw_applescript=false`)"
            ));
        }
        self.run_applescript_raw_internal(script, timeout_seconds)
    }

    pub fn show_notification(
        &self,
        title: &str,
        body: &str,
        subtitle: Option<&str>,
        timeout_seconds: Option<u64>,
    ) -> Result<ExecutionResult<MacOsExecutionData>> {
        let title = title.trim();
        let body = body.trim();
        if title.is_empty() {
            return Err(anyhow!("title is required"));
        }
        if body.is_empty() {
            return Err(anyhow!("body is required"));
        }

        #[cfg(target_os = "macos")]
        match self.run_native_notification(title, body, subtitle) {
            Ok(out) => return Ok(out),
            Err(err) => {
                warn!(
                    target: "borg_macos",
                    error = %err,
                    "native macOS notification failed; falling back to osascript"
                );
            }
        }

        let escaped_title = escape_applescript_string(title);
        let escaped_body = escape_applescript_string(body);
        let script =
            if let Some(subtitle) = subtitle.map(str::trim).filter(|value| !value.is_empty()) {
                let escaped_subtitle = escape_applescript_string(subtitle);
                format!(
                    "display notification \"{}\" with title \"{}\" subtitle \"{}\"",
                    escaped_body, escaped_title, escaped_subtitle
                )
            } else {
                format!(
                    "display notification \"{}\" with title \"{}\"",
                    escaped_body, escaped_title
                )
            };

        self.run_applescript_raw_internal(&script, timeout_seconds)
    }

    pub fn notify_status(
        &self,
        title: &str,
        body: &str,
        severity: Option<&str>,
        iphone_shortcut_name: Option<&str>,
        timeout_seconds: Option<u64>,
    ) -> Result<ExecutionResult<MacOsExecutionData>> {
        let started = Instant::now();
        let severity = severity.map(str::trim).filter(|value| !value.is_empty());
        let subtitle = severity.map(|value| format!("severity: {}", value));

        let local_notification =
            self.show_notification(title, body, subtitle.as_deref(), timeout_seconds)?;

        let relay_shortcut = iphone_shortcut_name
            .map(str::trim)
            .filter(|value| !value.is_empty());

        let mut relay_exit_code = None;
        let mut relay_stdout = None;
        if let Some(shortcut) = relay_shortcut {
            let relay = self.run_shortcut(shortcut, timeout_seconds)?;
            relay_exit_code = Some(relay.result.exit_code);
            relay_stdout = Some(relay.result.stdout);
        }

        let summary = json!({
            "title": title,
            "severity": severity,
            "local_notification": {
                "exit_code": local_notification.result.exit_code,
                "success": local_notification.result.success
            },
            "iphone_relay": {
                "shortcut_name": relay_shortcut,
                "exit_code": relay_exit_code,
                "stdout": relay_stdout
            }
        });
        let stdout = serde_json::to_string(&summary)?;
        let success = local_notification.result.success;
        let exit_code = if success { 0 } else { 1 };

        Ok(ExecutionResult {
            stdout: stdout.clone(),
            stderr: String::new(),
            result: MacOsExecutionData {
                command: "macos.notify_status".to_string(),
                exit_code,
                success,
                stdout,
                stderr: String::new(),
            },
            duration: started.elapsed(),
        })
    }

    pub fn open_target(
        &self,
        target: &str,
        timeout_seconds: Option<u64>,
    ) -> Result<ExecutionResult<MacOsExecutionData>> {
        let target = target.trim();
        if target.is_empty() {
            return Err(anyhow!("target is required"));
        }
        let args = vec![target.to_string()];
        self.run_command_with_timeout("open", &args, timeout_seconds)
    }

    pub fn say_text(
        &self,
        text: &str,
        voice: Option<&str>,
        timeout_seconds: Option<u64>,
    ) -> Result<ExecutionResult<MacOsExecutionData>> {
        let text = text.trim();
        if text.is_empty() {
            return Err(anyhow!("text is required"));
        }

        let mut args = Vec::new();
        if let Some(voice) = voice.map(str::trim).filter(|value| !value.is_empty()) {
            args.push("-v".to_string());
            args.push(voice.to_string());
        }
        args.push(text.to_string());
        self.run_command_with_timeout("say", &args, timeout_seconds)
    }

    fn run_applescript_raw_internal(
        &self,
        script: &str,
        timeout_seconds: Option<u64>,
    ) -> Result<ExecutionResult<MacOsExecutionData>> {
        let script = script.trim();
        if script.is_empty() {
            return Err(anyhow!("script is required"));
        }
        let timeout = self.resolve_timeout(timeout_seconds);
        let wrapped = wrap_applescript_with_timeout(script, timeout);
        let args = vec!["-e".to_string(), wrapped];
        self.run_command_with_timeout("osascript", &args, Some(timeout + 2))
    }

    fn resolve_timeout(&self, timeout_seconds: Option<u64>) -> u64 {
        let policy_limit = if self.policy.max_execution_seconds == 0 {
            DEFAULT_TIMEOUT_SECONDS
        } else {
            self.policy.max_execution_seconds
        };

        match timeout_seconds {
            Some(value) => value.clamp(1, policy_limit),
            None => policy_limit,
        }
    }

    fn run_command_with_timeout(
        &self,
        program: &str,
        args: &[String],
        timeout_seconds: Option<u64>,
    ) -> Result<ExecutionResult<MacOsExecutionData>> {
        let timeout = self.resolve_timeout(timeout_seconds);
        let command = render_command(program, args);
        info!(
            target: "borg_macos",
            command = %command,
            timeout_seconds = timeout,
            "executing macOS command"
        );

        let mut child = Command::new(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to spawn command `{}`", command))?;

        let started = Instant::now();
        let deadline = started + Duration::from_secs(timeout);

        loop {
            if let Some(status) = child
                .try_wait()
                .context("failed waiting for child status")?
            {
                let output = child
                    .wait_with_output()
                    .context("failed to read command output")?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = status.code().unwrap_or(-1);
                let duration = started.elapsed();
                debug!(
                    target: "borg_macos",
                    command = %command,
                    exit_code,
                    stdout_len = stdout.len(),
                    stderr_len = stderr.len(),
                    "macOS command completed"
                );
                return Ok(ExecutionResult {
                    stdout: stdout.clone(),
                    stderr: stderr.clone(),
                    result: MacOsExecutionData {
                        command,
                        exit_code,
                        success: status.success(),
                        stdout,
                        stderr,
                    },
                    duration,
                });
            }

            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow!("command timed out after {}s: {}", timeout, command));
            }

            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[cfg(target_os = "macos")]
    fn run_native_notification(
        &self,
        title: &str,
        body: &str,
        subtitle: Option<&str>,
    ) -> Result<ExecutionResult<MacOsExecutionData>> {
        let started = Instant::now();
        send_notification(title, subtitle, body, None)
            .map_err(|err| anyhow!("native notification delivery failed: {}", err))?;

        let stdout = "native notification delivered".to_string();
        let stderr = String::new();
        Ok(ExecutionResult {
            stdout: stdout.clone(),
            stderr: stderr.clone(),
            result: MacOsExecutionData {
                command: "mac_notification_sys::send_notification".to_string(),
                exit_code: 0,
                success: true,
                stdout,
                stderr,
            },
            duration: started.elapsed(),
        })
    }
}

fn render_template_script(template_id: &str, parameters: &Value) -> Result<String> {
    match template_id {
        "system.display_notification" => {
            let title = required_string(parameters, "title")?;
            let body = required_string(parameters, "body")?;
            let subtitle = optional_string(parameters, "subtitle");
            let escaped_title = escape_applescript_string(title);
            let escaped_body = escape_applescript_string(body);
            if let Some(subtitle) = subtitle {
                let escaped_subtitle = escape_applescript_string(subtitle);
                Ok(format!(
                    "display notification \"{}\" with title \"{}\" subtitle \"{}\"",
                    escaped_body, escaped_title, escaped_subtitle
                ))
            } else {
                Ok(format!(
                    "display notification \"{}\" with title \"{}\"",
                    escaped_body, escaped_title
                ))
            }
        }
        "shell.echo" => {
            let message = required_string(parameters, "message")?;
            let escaped_message = message.replace("\\", "\\\\").replace("\"", "\\\"");
            Ok(format!("do shell script \"echo {}\"", escaped_message))
        }
        _ => Err(anyhow!("unknown template_id `{}`", template_id)),
    }
}

fn required_string<'a>(parameters: &'a Value, key: &str) -> Result<&'a str> {
    parameters
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("template parameter `{}` is required", key))
}

fn optional_string<'a>(parameters: &'a Value, key: &str) -> Option<&'a str> {
    parameters
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn render_command(program: &str, args: &[String]) -> String {
    if args.is_empty() {
        return program.to_string();
    }
    format!("{} {}", program, args.join(" "))
}

pub fn escape_applescript_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn wrap_applescript_with_timeout(script: &str, timeout_seconds: u64) -> String {
    let timeout_seconds = timeout_seconds.max(1);
    let mut out = format!("with timeout of {} seconds\n", timeout_seconds);
    for line in script.lines() {
        out.push_str("  ");
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("end timeout");
    out
}
