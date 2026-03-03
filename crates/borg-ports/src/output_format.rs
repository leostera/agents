use serde::Serialize;

pub fn format_tool_action_message<T: Serialize>(tool_name: &str, arguments: &T) -> String {
    let label = match tool_name {
        "CodeMode-executeCode" => "Running code",
        "CodeMode-searchApis" => "Searching APIs",
        "Memory-getSchema" => "Loading memory schema",
        "Memory-searchMemory" => "Searching memory",
        "Memory-storeFacts" => "Storing memory",
        "TaskGraph-createTask" => "Creating task",
        "TaskGraph-updateTask" => "Updating task",
        "TaskGraph-setTaskStatus" => "Updating task status",
        "TaskGraph-submitReview" => "Submitting review",
        "TaskGraph-approveReview" => "Approving review",
        "TaskGraph-requestReviewChanges" => "Requesting review changes",
        _ => "Running tool",
    };
    let pretty_args =
        serde_json::to_string_pretty(arguments).unwrap_or_else(|_| "<invalid_args>".to_string());
    format!("Action: {label}\n{}", pretty_args.trim())
}

pub fn split_message_by_limit(message: &str, message_limit: usize) -> Vec<String> {
    if message.is_empty() || message_limit == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut current = String::new();
    for line in message.lines() {
        if line.len() > message_limit {
            if !current.trim().is_empty() {
                out.push(current.trim_end().to_string());
                current.clear();
            }
            let mut start = 0usize;
            while start < line.len() {
                let end = (start + message_limit).min(line.len());
                out.push(line[start..end].to_string());
                start = end;
            }
            continue;
        }
        if current.len() + line.len() + 1 > message_limit {
            out.push(current.trim_end().to_string());
            current.clear();
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        out.push(current.trim_end().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::split_message_by_limit;

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
}
