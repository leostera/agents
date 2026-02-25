use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use serde_json::{Value, json};

use crate::CodeModeRuntime;

#[test]
fn executes_with_injected_sdk_and_ffi() {
    let rt = CodeModeRuntime::default();
    let result = rt.execute("Borg.OS.ls('.')").unwrap();
    assert!(result.result_json.is_object());
    let entries = result
        .result_json
        .get("entries")
        .and_then(Value::as_array)
        .unwrap();
    assert!(!entries.is_empty());
}

#[test]
fn custom_ffi_handler_can_override_sdk_behavior() {
    let rt =
        CodeModeRuntime::default().with_ffi_handler("os__ls", |args| Ok(json!({ "args": args })));
    let result = rt.execute("Borg.OS.ls('a', 'b')").unwrap();
    assert_eq!(result.result_json, json!({ "args": ["a", "b"] }));
}

#[test]
fn fetch_uses_net_ffi_and_returns_response_shape() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}/hello", addr);

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0_u8; 2048];
        let _ = stream.read(&mut buf).unwrap();
        let response_body = r#"{"ok":true,"source":"test-server"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });

    let rt = CodeModeRuntime::default();
    let result = rt
        .execute(&format!(
            "Borg.fetch('{}', {{ method: 'GET', headers: {{ 'x-test': '1' }} }})",
            url
        ))
        .unwrap();

    assert_eq!(result.result_json.get("status"), Some(&json!(200)));
    assert_eq!(result.result_json.get("ok"), Some(&json!(true)));
    assert_eq!(
        result
            .result_json
            .get("json")
            .and_then(|v| v.get("source"))
            .and_then(Value::as_str),
        Some("test-server")
    );

    server.join().unwrap();
}
