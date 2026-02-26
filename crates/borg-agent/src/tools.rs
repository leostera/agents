use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use borg_llm::ToolDescriptor;
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

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
    Execution {
        result: Value,
        #[serde(
            alias = "duration_ms",
            deserialize_with = "deserialize_duration_compat"
        )]
        duration: Duration,
    },
    Error {
        message: String,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DurationCompat {
    Structured { secs: u64, nanos: u32 },
    Millis(u64),
}

fn deserialize_duration_compat<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    match DurationCompat::deserialize(deserializer)? {
        DurationCompat::Structured { secs, nanos } => Ok(Duration::new(secs, nanos)),
        DurationCompat::Millis(ms) => Ok(Duration::from_millis(ms)),
    }
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

pub fn to_provider_tool_specs(tool_specs: &[ToolSpec]) -> Vec<ToolDescriptor> {
    tool_specs.iter().map(ToolDescriptor::from).collect()
}

type ToolFuture = Pin<Box<dyn Future<Output = Result<ToolResponse>> + Send>>;
type ToolCallback = Arc<dyn Fn(ToolRequest) -> ToolFuture + Send + Sync>;

pub struct Tool {
    pub spec: ToolSpec,
    pub output_schema: Option<Value>,
    callback: ToolCallback,
}

impl Tool {
    pub fn new<F, Fut>(spec: ToolSpec, output_schema: Option<Value>, callback: F) -> Self
    where
        F: Fn(ToolRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ToolResponse>> + Send + 'static,
    {
        Self {
            spec,
            output_schema,
            callback: Arc::new(move |request| Box::pin(callback(request))),
        }
    }
}

pub struct Toolchain {
    tools: HashMap<String, Tool>,
}

impl Toolchain {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn builder() -> ToolchainBuilder {
        ToolchainBuilder::new()
    }

    pub fn register(&mut self, tool: Tool) -> Result<()> {
        let name = tool.spec.name.clone();
        if self.tools.contains_key(&name) {
            return Err(anyhow!("tool already registered: {}", name));
        }
        self.tools.insert(name, tool);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn merge(mut self, other: Toolchain) -> Result<Self> {
        for (name, tool) in other.tools {
            if self.tools.contains_key(&name) {
                return Err(anyhow!("tool already registered: {}", name));
            }
            self.tools.insert(name, tool);
        }
        Ok(self)
    }
}

impl Default for Toolchain {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolRunner for Toolchain {
    async fn run(&self, request: ToolRequest) -> Result<ToolResponse> {
        let Some(tool) = self.tools.get(&request.tool_name) else {
            return Err(anyhow!("unknown tool {}", request.tool_name));
        };

        validate_schema(
            &request.arguments,
            &tool.spec.parameters,
            &format!("tool:{}:input", tool.spec.name),
        )?;

        let response = (tool.callback)(request).await?;
        if let Some(output_schema) = &tool.output_schema {
            let output_value = serde_json::to_value(&response.content)?;
            validate_schema(
                &output_value,
                output_schema,
                &format!("tool:{}:output", tool.spec.name),
            )?;
        }

        Ok(response)
    }
}

pub struct ToolchainBuilder {
    toolchain: Toolchain,
}

impl ToolchainBuilder {
    pub fn new() -> Self {
        Self {
            toolchain: Toolchain::new(),
        }
    }

    pub fn add_tool(mut self, tool: Tool) -> Result<Self> {
        self.toolchain.register(tool)?;
        Ok(self)
    }

    pub fn build(self) -> Result<Toolchain> {
        Ok(self.toolchain)
    }
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

fn validate_schema(value: &Value, schema: &Value, path: &str) -> Result<()> {
    let Some(expected_type) = schema.get("type").and_then(Value::as_str) else {
        return Ok(());
    };

    match expected_type {
        "object" => {
            let object = value
                .as_object()
                .ok_or_else(|| anyhow!("{} expected object, got {}", path, value_type(value)))?;

            if let Some(required) = schema.get("required").and_then(Value::as_array) {
                for key in required {
                    if let Some(key) = key.as_str() {
                        if !object.contains_key(key) {
                            return Err(anyhow!("{} missing required property `{}`", path, key));
                        }
                    }
                }
            }

            let allow_additional = schema
                .get("additionalProperties")
                .and_then(Value::as_bool)
                .unwrap_or(true);

            let properties = schema
                .get("properties")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();

            for (key, val) in object {
                if let Some(prop_schema) = properties.get(key) {
                    validate_schema(val, prop_schema, &format!("{}.{}", path, key))?;
                } else if !allow_additional {
                    return Err(anyhow!("{} has unexpected property `{}`", path, key));
                }
            }
            Ok(())
        }
        "string" => {
            if value.is_string() {
                Ok(())
            } else {
                Err(anyhow!(
                    "{} expected string, got {}",
                    path,
                    value_type(value)
                ))
            }
        }
        "number" => {
            if value.is_number() {
                Ok(())
            } else {
                Err(anyhow!(
                    "{} expected number, got {}",
                    path,
                    value_type(value)
                ))
            }
        }
        "boolean" => {
            if value.is_boolean() {
                Ok(())
            } else {
                Err(anyhow!(
                    "{} expected boolean, got {}",
                    path,
                    value_type(value)
                ))
            }
        }
        "array" => {
            if value.is_array() {
                Ok(())
            } else {
                Err(anyhow!(
                    "{} expected array, got {}",
                    path,
                    value_type(value)
                ))
            }
        }
        _ => Ok(()),
    }
}

fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
