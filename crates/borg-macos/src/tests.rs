use serde_json::json;

use crate::{MacOsRuntime, escape_applescript_string, wrap_applescript_with_timeout};

#[test]
fn escape_applescript_string_escapes_quotes_and_backslashes() {
    let value = r#"a "quote" and \ slash"#;
    let out = escape_applescript_string(value);
    assert_eq!(out, r#"a \"quote\" and \\ slash"#);
}

#[test]
fn wrap_applescript_with_timeout_wraps_script_block() {
    let script = "display notification \"hello\" with title \"borg\"";
    let out = wrap_applescript_with_timeout(script, 5);
    assert!(out.starts_with("with timeout of 5 seconds"));
    assert!(out.contains("  display notification"));
    assert!(out.ends_with("end timeout"));
}

#[test]
fn raw_applescript_is_disabled_by_default() {
    let rt = MacOsRuntime::new();
    let err = rt
        .run_applescript_raw("return \"ok\"", Some(1))
        .expect_err("raw applescript should be blocked by default");
    assert!(err.to_string().contains("disabled by policy"));
}

#[test]
fn unknown_template_returns_error() {
    let rt = MacOsRuntime::new();
    let err = rt
        .run_applescript_template("missing.template", &json!({}), Some(1))
        .expect_err("unknown templates should fail");
    assert!(err.to_string().contains("unknown template_id"));
}

#[cfg(target_os = "macos")]
#[test]
fn list_shortcuts_smoke() {
    let rt = MacOsRuntime::new();
    let out = rt
        .list_shortcuts(None, false, true, Some(5))
        .expect("expected shortcuts list to run");
    assert!(out.result.exit_code == 0 || out.result.exit_code == 1);
}

#[cfg(target_os = "macos")]
#[test]
fn notification_template_smoke() {
    let rt = MacOsRuntime::new();
    let out = rt
        .run_applescript_template(
            "system.display_notification",
            &json!({ "title": "Borg", "body": "macos spike test" }),
            Some(5),
        )
        .expect("expected template notification to run");
    assert_eq!(out.result.exit_code, 0);
}

#[cfg(target_os = "macos")]
#[test]
fn notify_status_smoke_without_iphone_relay() {
    let rt = MacOsRuntime::new();
    let out = rt
        .notify_status(
            "Borg Spike",
            "status is available",
            Some("info"),
            None,
            Some(5),
        )
        .expect("expected notify_status to run");
    assert_eq!(out.result.exit_code, 0);
}
