use anyhow::{Result, anyhow};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

const BORG_SUGGESTION_MEMORY: &str = "Hint: use `Borg.Memory.*`, not `Borg.LTM.*`.";
const BORG_ALLOWED_ROOTS: &[&str] = &["OS", "Memory", "URI", "fetch"];
const BORG_ALLOWED_MEMORY_METHODS: &[&str] = &["stateFacts", "search"];
const BORG_ALLOWED_OS_METHODS: &[&str] = &["ls"];
const BORG_ALLOWED_URI_METHODS: &[&str] = &["new", "parse"];

pub fn precheck_borg_sdk_usage(code: &str) -> Result<()> {
    validate_javascript_syntax(code)?;
    validate_borg_surface(code)
}

fn validate_javascript_syntax(code: &str) -> Result<()> {
    let allocator = Allocator::default();
    let parse = Parser::new(&allocator, code, SourceType::mjs()).parse();
    if parse.errors.is_empty() {
        return Ok(());
    }
    let first = format!("{:?}", parse.errors[0]);
    Err(anyhow!("code failed pre-execution parse check: {first}"))
}

fn validate_borg_surface(code: &str) -> Result<()> {
    for access in extract_borg_accesses(code) {
        if access.is_empty() {
            continue;
        }
        let root = access[0].as_str();
        if !BORG_ALLOWED_ROOTS.contains(&root) {
            if root == "LTM" {
                return Err(anyhow!(
                    "invalid Borg SDK namespace `Borg.{root}`. {BORG_SUGGESTION_MEMORY}"
                ));
            }
            return Err(anyhow!(
                "invalid Borg SDK namespace `Borg.{root}`. Allowed roots: Borg.OS, Borg.Memory, Borg.URI, Borg.fetch"
            ));
        }

        if root == "Memory" && access.len() >= 2 {
            let method = access[1].as_str();
            if !BORG_ALLOWED_MEMORY_METHODS.contains(&method) {
                return Err(anyhow!(
                    "invalid Borg.Memory API `Borg.Memory.{method}`. Allowed methods: stateFacts, search"
                ));
            }
        }
        if root == "OS" && access.len() >= 2 {
            let method = access[1].as_str();
            if !BORG_ALLOWED_OS_METHODS.contains(&method) {
                return Err(anyhow!(
                    "invalid Borg.OS API `Borg.OS.{method}`. Allowed methods: ls"
                ));
            }
        }
        if root == "URI" && access.len() >= 2 {
            let method = access[1].as_str();
            if !BORG_ALLOWED_URI_METHODS.contains(&method) {
                return Err(anyhow!(
                    "invalid Borg.URI API `Borg.URI.{method}`. Allowed methods: new, parse"
                ));
            }
        }
    }
    Ok(())
}

fn extract_borg_accesses(code: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let bytes = code.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        let Some(rel) = code[i..].find("Borg.") else {
            break;
        };
        let start = i + rel;
        if start > 0 && is_ident(bytes[start - 1] as char) {
            i = start + 5;
            continue;
        }

        let mut cursor = start + "Borg.".len();
        let mut segments = Vec::new();
        while cursor < bytes.len() {
            let Some((segment, end)) = read_ident(code, cursor) else {
                break;
            };
            segments.push(segment.to_string());
            cursor = end;
            if cursor >= bytes.len() || bytes[cursor] as char != '.' {
                break;
            }
            cursor += 1;
        }

        if !segments.is_empty() {
            out.push(segments);
        }
        i = start + "Borg.".len();
    }

    out
}

fn read_ident(input: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = input.as_bytes();
    if start >= bytes.len() {
        return None;
    }
    let first = bytes[start] as char;
    if !is_ident_start(first) {
        return None;
    }

    let mut end = start + 1;
    while end < bytes.len() {
        let c = bytes[end] as char;
        if !is_ident(c) {
            break;
        }
        end += 1;
    }
    Some((&input[start..end], end))
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

