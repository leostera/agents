use anyhow::Result;
use async_trait::async_trait;
use borg_llm::ToolDescriptor;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    pub content: ToolResultData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySummary {
    pub name: String,
    pub signature: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolResultData {
    Text(String),
    Capabilities(Vec<CapabilitySummary>),
    Execution { result: String, duration_ms: u128 },
    Error { message: String },
}

#[async_trait]
pub trait ToolRunner: Send + Sync {
    async fn run(&self, request: ToolRequest) -> Result<ToolResponse>;
}

pub struct AgentTools<'a> {
    pub tool_runner: &'a dyn ToolRunner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl From<&ToolSpec> for ToolDescriptor {
    fn from(value: &ToolSpec) -> Self {
        Self {
            name: value.name.clone(),
            description: value.description.clone(),
            input_schema: value.parameters.clone(),
        }
    }
}

pub fn default_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "search".to_string(),
            description: "Search capabilities or memory context for the user request".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "execute".to_string(),
            description: "Execute runtime code/action for task fulfillment".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "code": { "type": "string" } },
                "required": ["code"],
                "additionalProperties": false
            }),
        },
    ]
}

pub fn to_provider_tool_specs(tool_specs: &[ToolSpec]) -> Vec<ToolDescriptor> {
    tool_specs.iter().map(ToolDescriptor::from).collect()
}

pub async fn call_tool<'a>(
    tools: &AgentTools<'a>,
    tool_call_id: &str,
    tool_name: &str,
    arguments: &Value,
) -> Result<ToolResultData> {
    let response = tools
        .tool_runner
        .run(ToolRequest {
            tool_call_id: tool_call_id.to_string(),
            tool_name: tool_name.to_string(),
            arguments: arguments.clone(),
        })
        .await?;
    Ok(response.content)
}
