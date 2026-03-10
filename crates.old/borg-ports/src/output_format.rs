use serde::Serialize;
use std::time::Duration;

pub fn format_tool_action_message<T: Serialize>(tool_name: &str, arguments: &T) -> String {
    let raw_args = serde_json::to_string(arguments).ok();
    let label = raw_args
        .as_deref()
        .and_then(extract_hint)
        .unwrap_or_else(|| tool_name.to_string());
    let pretty_args =
        serde_json::to_string_pretty(arguments).unwrap_or_else(|_| "<invalid_args>".to_string());
    format!("Action: {label}\n{}", pretty_args.trim())
}

const TELEGRAM_TOOL_ACTION_DETAILS_MAX_CHARS: usize = 1_500;

pub fn format_tool_action_message_for_telegram_html<T: Serialize>(
    tool_name: &str,
    arguments: &T,
    duration: Option<Duration>,
) -> String {
    let raw_args = serde_json::to_string(arguments).ok();
    let label = raw_args
        .as_deref()
        .and_then(extract_hint)
        .unwrap_or_else(|| tool_name.to_string());
    let pretty_args =
        serde_json::to_string_pretty(arguments).unwrap_or_else(|_| "<invalid_args>".to_string());
    let elapsed = format_elapsed_time(duration);
    let details = truncate_chars(pretty_args.trim(), TELEGRAM_TOOL_ACTION_DETAILS_MAX_CHARS);
    let escaped_label = escape_html_text(&label);
    let escaped_details = escape_html_text(&details);
    format!(
        "<i>{escaped_label}</i> ({elapsed})\nSee details: <tg-spoiler>{escaped_details}</tg-spoiler>"
    )
}

#[derive(serde::Deserialize)]
struct ToolActionHintOnly {
    hint: Option<String>,
}

fn extract_hint(arguments_json: &str) -> Option<String> {
    let parsed: ToolActionHintOnly = serde_json::from_str(arguments_json).ok()?;
    parsed
        .hint
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn format_elapsed_time(duration: Option<Duration>) -> String {
    const ONE_SECOND_MS: u128 = 1_000;
    const TEN_SECONDS: f64 = 10.0;

    match duration {
        Some(duration) if duration.as_millis() < ONE_SECOND_MS => {
            format!("{}ms", duration.as_millis())
        }
        Some(duration) if duration.as_secs_f64() < TEN_SECONDS => {
            format!("{:.1}s", duration.as_secs_f64())
        }
        Some(duration) => format!("{}s", duration.as_secs()),
        None => "unknown".to_string(),
    }
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = input.chars().count();
    if char_count <= max_chars {
        return input.to_string();
    }

    const ELLIPSIS: &str = "...";
    let keep = max_chars.saturating_sub(ELLIPSIS.chars().count());
    let mut out: String = input.chars().take(keep).collect();
    out.push_str(ELLIPSIS);
    out
}

pub fn format_for_telegram_html(message: &str) -> String {
    if message.trim().is_empty() {
        return String::new();
    }

    let mut out = Vec::new();
    for line in message.lines() {
        if line.trim().is_empty() {
            out.push(String::new());
            continue;
        }

        let leading_whitespace = line
            .char_indices()
            .find_map(|(idx, ch)| (!ch.is_whitespace()).then_some(idx))
            .unwrap_or(line.len());
        let indent = &line[..leading_whitespace];
        let trimmed = &line[leading_whitespace..];

        if let Some(item) = list_item(trimmed) {
            out.push(format!(
                "{indent}• {}",
                format_inline_for_telegram_html(item, 0)
            ));
            continue;
        }

        out.push(format_inline_for_telegram_html(line, 0));
    }

    out.join("\n")
}

fn list_item(line: &str) -> Option<&str> {
    for prefix in ["- ", "* ", "+ "] {
        if let Some(item) = line.strip_prefix(prefix) {
            return Some(item);
        }
    }
    None
}

fn format_inline_for_telegram_html(input: &str, depth: usize) -> String {
    const MAX_DEPTH: usize = 8;
    if depth > MAX_DEPTH {
        return escape_html_text(input);
    }

    let mut out = String::new();
    let mut i = 0usize;

    while i < input.len() {
        if input[i..].starts_with("**")
            && let Some((content, consumed)) = enclosed_segment(&input[i + 2..], "**")
        {
            out.push_str("<b>");
            out.push_str(&format_inline_for_telegram_html(content, depth + 1));
            out.push_str("</b>");
            i += 2 + consumed;
            continue;
        }

        if input[i..].starts_with('*')
            && let Some((content, consumed)) = enclosed_segment(&input[i + 1..], "*")
        {
            out.push_str("<i>");
            out.push_str(&format_inline_for_telegram_html(content, depth + 1));
            out.push_str("</i>");
            i += 1 + consumed;
            continue;
        }

        if input[i..].starts_with('_')
            && let Some((content, consumed)) = enclosed_segment(&input[i + 1..], "_")
        {
            out.push_str("<i>");
            out.push_str(&format_inline_for_telegram_html(content, depth + 1));
            out.push_str("</i>");
            i += 1 + consumed;
            continue;
        }

        if input[i..].starts_with('`')
            && let Some((content, consumed)) = enclosed_segment(&input[i + 1..], "`")
        {
            out.push_str("<code>");
            out.push_str(&escape_html_text(content));
            out.push_str("</code>");
            i += 1 + consumed;
            continue;
        }

        if input[i..].starts_with('[')
            && let Some((label, href, consumed)) = markdown_link(&input[i..])
        {
            out.push_str("<a href=\"");
            out.push_str(&escape_html_attr(href));
            out.push_str("\">");
            out.push_str(&format_inline_for_telegram_html(label, depth + 1));
            out.push_str("</a>");
            i += consumed;
            continue;
        }

        if let Some(ch) = input[i..].chars().next() {
            append_escaped_char(&mut out, ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }

    out
}

fn enclosed_segment<'a>(input: &'a str, delimiter: &str) -> Option<(&'a str, usize)> {
    let closing_idx = input.find(delimiter)?;
    let content = &input[..closing_idx];
    if content.trim().is_empty() {
        return None;
    }
    Some((content, closing_idx + delimiter.len()))
}

fn markdown_link(input: &str) -> Option<(&str, &str, usize)> {
    let close_label = input.find(']')?;
    let after_label = input.get(close_label + 1..)?;
    if !after_label.starts_with('(') {
        return None;
    }
    let close_href = after_label.find(')')?;
    let label = input.get(1..close_label)?.trim();
    let href = after_label.get(1..close_href)?.trim();
    if label.is_empty() || href.is_empty() || !is_safe_href(href) {
        return None;
    }
    let consumed = close_label + 1 + close_href + 1;
    Some((label, href, consumed))
}

fn is_safe_href(href: &str) -> bool {
    let lower = href.trim().to_ascii_lowercase();
    lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("tg://")
        || lower.starts_with("mailto:")
}

fn escape_html_text(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        append_escaped_char(&mut out, ch);
    }
    out
}

fn escape_html_attr(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn append_escaped_char(out: &mut String, ch: char) {
    match ch {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        _ => out.push(ch),
    }
}

pub fn split_message_by_limit(message: &str, message_limit: usize) -> Vec<String> {
    if message.is_empty() || message_limit == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for line in message.lines() {
        let line_len = line.chars().count();
        if line_len > message_limit {
            if !current.trim().is_empty() {
                out.push(current.trim_end().to_string());
                current.clear();
                current_len = 0;
            }
            let mut chunk = String::new();
            let mut chunk_len = 0usize;
            for ch in line.chars() {
                chunk.push(ch);
                chunk_len += 1;
                if chunk_len == message_limit {
                    out.push(chunk);
                    chunk = String::new();
                    chunk_len = 0;
                }
            }
            if !chunk.is_empty() {
                out.push(chunk);
            }
            continue;
        }
        if current_len + line_len + 1 > message_limit {
            out.push(current.trim_end().to_string());
            current.clear();
            current_len = 0;
        }
        current.push_str(line);
        current.push('\n');
        current_len += line_len + 1;
    }
    if !current.trim().is_empty() {
        out.push(current.trim_end().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        format_for_telegram_html, format_tool_action_message,
        format_tool_action_message_for_telegram_html, split_message_by_limit,
    };
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn split_respects_limit() {
        let parts = split_message_by_limit("abc\ndef\nghi", 5);
        assert_eq!(parts, vec!["abc", "def", "ghi"]);
    }

    #[test]
    fn split_breaks_long_lines_without_empty_chunks() {
        let parts = split_message_by_limit("abcdefghij", 4);
        assert_eq!(parts, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn split_breaks_unicode_long_lines_without_panics() {
        let parts = split_message_by_limit("😀😀😀😀😀", 2);
        assert_eq!(parts, vec!["😀😀", "😀😀", "😀"]);
    }

    #[test]
    fn format_tool_action_uses_hint_label() {
        let message = format_tool_action_message(
            "ShellMode-executeCommand",
            &json!({
                "hint": "Checking git status for uncommitted changes",
                "command": "git status --porcelain"
            }),
        );
        assert!(message.starts_with("Action: Checking git status for uncommitted changes\n"));
    }

    #[test]
    fn format_tool_action_falls_back_to_tool_name_without_hint() {
        let message = format_tool_action_message(
            "ShellMode-executeCommand",
            &json!({ "command": "git status --porcelain" }),
        );
        assert!(message.starts_with("Action: ShellMode-executeCommand\n"));
    }

    #[test]
    fn format_tool_action_for_telegram_html_includes_hint_duration_and_spoiler() {
        let message = format_tool_action_message_for_telegram_html(
            "ShellMode-executeCommand",
            &json!({
                "hint": "Checking git status for uncommitted changes",
                "command": "git status --porcelain"
            }),
            Some(Duration::from_millis(240)),
        );
        assert!(message.contains("<i>Checking git status for uncommitted changes</i> (240ms)"));
        assert!(message.contains("See details: <tg-spoiler>{"));
        assert!(message.contains("\"command\": \"git status --porcelain\""));
    }

    #[test]
    fn format_tool_action_for_telegram_html_falls_back_when_duration_missing() {
        let message = format_tool_action_message_for_telegram_html(
            "ShellMode-executeCommand",
            &json!({ "command": "git status --porcelain" }),
            None,
        );
        assert!(message.starts_with("<i>ShellMode-executeCommand</i> (unknown)\n"));
    }

    #[test]
    fn format_for_telegram_html_renders_supported_markdown() {
        let formatted = format_for_telegram_html(
            "Hello **friend**.\n- *One*\n- [Two](https://example.com)\n`inline`",
        );
        assert!(formatted.contains("<b>friend</b>"));
        assert!(formatted.contains("• <i>One</i>"));
        assert!(formatted.contains("• <a href=\"https://example.com\">Two</a>"));
        assert!(formatted.contains("<code>inline</code>"));
    }

    #[test]
    fn format_for_telegram_html_escapes_raw_html() {
        let formatted = format_for_telegram_html("<b>not bold</b> & <i>not italic</i>");
        assert_eq!(
            formatted,
            "&lt;b&gt;not bold&lt;/b&gt; &amp; &lt;i&gt;not italic&lt;/i&gt;"
        );
    }

    #[test]
    fn format_for_telegram_html_drops_unsafe_links() {
        let formatted = format_for_telegram_html("[click](javascript:alert(1))");
        assert_eq!(formatted, "[click](javascript:alert(1))");
    }
}
