use borg_exec::{RuntimeToolCall, RuntimeToolResult, ToolCallSummary};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ExecuteCodeArgs {
    hint: Option<String>,
    code: Option<String>,
}

pub fn format_tool_action_message(
    call: &ToolCallSummary<RuntimeToolCall, RuntimeToolResult>,
) -> String {
    let tool_name = call.tool_name.as_str();
    let parsed_args = call.arguments.to_value().ok();
    let raw_args = parsed_args
        .as_ref()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "<invalid_args>".to_string());
    let tool_label = humanize_tool_name(tool_name);

    if tool_name == "CodeMode-executeCode" {
        let execute_args = parsed_args
            .as_ref()
            .and_then(|args| serde_json::from_value::<ExecuteCodeArgs>(args.clone()).ok());
        let hinted_title = execute_args
            .as_ref()
            .and_then(|args| args.hint.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(title) = hinted_title else {
            return format!(
                "Action: {}\n{}",
                "Invalid execute call: missing required `hint`",
                execute_args
                    .as_ref()
                    .and_then(|args| args.code.as_deref())
                    .unwrap_or(raw_args.as_str())
                    .trim()
            );
        };
        if let Some(code) = execute_args
            .as_ref()
            .and_then(|args| args.code.as_deref())
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
