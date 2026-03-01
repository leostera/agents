use borg_exec::ToolCallSummary;
use serde_json::Value;

pub fn format_tool_action_message(call: &ToolCallSummary) -> String {
    let tool_name = call.tool_name.as_str();
    let raw_args = call.arguments.to_string();
    let tool_label = humanize_tool_name(tool_name);
    let parsed_args = Some(call.arguments.clone());

    if tool_name == "CodeMode-executeCode" {
        let hinted_title = parsed_args
            .as_ref()
            .and_then(|args| args.get("hint"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(title) = hinted_title else {
            return format!(
                "Action: {}\n{}",
                "Invalid execute call: missing required `hint`",
                parsed_args
                    .as_ref()
                    .and_then(|args| args.get("code"))
                    .and_then(Value::as_str)
                    .unwrap_or(raw_args.as_str())
                    .trim()
            );
        };
        if let Some(code) = parsed_args
            .as_ref()
            .and_then(|args| args.get("code"))
            .and_then(Value::as_str)
        {
            return format!("Action: {title}\n{}", code.trim());
        }
        return format!("Action: {title}");
    }

    let pretty_args = parsed_args
        .as_ref()
        .and_then(|value| serde_json::to_string_pretty(value).ok())
        .unwrap_or_else(|| raw_args.to_string());
    format!("Action: {tool_label}\n{}", pretty_args.trim())
}

fn humanize_tool_name(tool_name: &str) -> &'static str {
    match tool_name {
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
    }
}
