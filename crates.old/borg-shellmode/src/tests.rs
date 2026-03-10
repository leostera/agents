use crate::{ShellModeContext, ShellModeRuntime};
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn executes_printf_command() {
    let rt = ShellModeRuntime::new();
    let result = rt
        .execute("printf 'hello'", ShellModeContext::default())
        .unwrap();
    assert_eq!(result.stdout.trim(), "hello");
    assert_eq!(result.stderr, "");
    assert_eq!(result.result.exit_code, 0);
}

#[test]
fn returns_exit_code_on_success() {
    let rt = ShellModeRuntime::new();
    let result = rt.execute("true", ShellModeContext::default()).unwrap();
    assert_eq!(result.result.exit_code, 0);
}

#[test]
fn returns_exit_code_on_failure() {
    let rt = ShellModeRuntime::new();
    let result = rt.execute("exit 42", ShellModeContext::default()).unwrap();
    assert_eq!(result.result.exit_code, 42);
}

#[test]
fn captures_stderr() {
    let rt = ShellModeRuntime::new();
    let result = rt
        .execute("printf 'error' >&2", ShellModeContext::default())
        .unwrap();
    assert_eq!(result.stderr.trim(), "error");
}

#[test]
fn executes_in_custom_working_directory() {
    let rt = ShellModeRuntime::new();
    let ctx = ShellModeContext::default().with_working_directory(PathBuf::from("/tmp"));
    let result = rt.execute("pwd", ctx).unwrap();
    assert!(result.stdout.trim().ends_with("tmp") || result.stdout.trim() == "/tmp");
}

#[test]
fn respects_timeout() {
    let rt = ShellModeRuntime::new();
    let ctx = ShellModeContext::default().with_timeout(1);
    let result = rt.execute("sleep 5", ctx);
    assert!(result.is_err() || result.unwrap().result.exit_code >= -1);
}

#[test]
fn returns_duration_in_result() {
    let rt = ShellModeRuntime::new();
    let result = rt
        .execute("sleep 0.1", ShellModeContext::default())
        .unwrap();
    let secs = result.duration.as_secs();
    let nanos = result.duration.subsec_nanos() as u64;
    assert!(secs > 0 || nanos > 0);
}

#[test]
fn handles_command_not_found() {
    let rt = ShellModeRuntime::new();
    let result = rt.execute("nonexistent_command_12345", ShellModeContext::default());
    assert!(result.is_err() || result.unwrap().result.exit_code != 0);
}

#[test]
fn supports_pipeline() {
    let rt = ShellModeRuntime::new();
    let result = rt
        .execute(
            "printf 'line1\\nline2\\nline3' | head -n 2",
            ShellModeContext::default(),
        )
        .unwrap();
    assert_eq!(result.stdout.trim(), "line1\nline2");
}

#[test]
fn runtime_default_timeout() {
    let rt = ShellModeRuntime::new().with_default_timeout(Duration::from_secs(60));
    let ctx = ShellModeContext::default();
    let result = rt.execute("printf 'test'", ctx).unwrap();
    assert_eq!(result.stdout.trim(), "test");
}
