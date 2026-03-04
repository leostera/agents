use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use borg_llm::ToolDescriptor;
use serde::de::{DeserializeOwned, Deserializer};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BorgToolCall {
    encoded_json: String,
}

impl BorgToolCall {
    pub fn to_value(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.encoded_json)?)
    }
}

impl From<Value> for BorgToolCall {
    fn from(value: Value) -> Self {
        Self {
            encoded_json: value.to_string(),
        }
    }
}

impl Serialize for BorgToolCall {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value: Value =
            serde_json::from_str(&self.encoded_json).map_err(serde::ser::Error::custom)?;
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BorgToolCall {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(Self {
            encoded_json: value.to_string(),
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BorgToolResult {
    encoded_json: String,
}

impl BorgToolResult {
    pub fn to_value(&self) -> Result<Value> {
        Ok(serde_json::from_str(&self.encoded_json)?)
    }
}

impl From<Value> for BorgToolResult {
    fn from(value: Value) -> Self {
        Self {
            encoded_json: value.to_string(),
        }
    }
}

impl Serialize for BorgToolResult {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value: Value =
            serde_json::from_str(&self.encoded_json).map_err(serde::ser::Error::custom)?;
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BorgToolResult {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(Self {
            encoded_json: value.to_string(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest<TToolCall> {
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: TToolCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse<TToolResult> {
    pub content: ToolResultData<TToolResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySummary {
    pub name: String,
    pub signature: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolResultData<TToolResult> {
    Text(String),
    Capabilities(Vec<CapabilitySummary>),
    Execution {
        result: TToolResult,
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

type TypedToolFuture<TToolResult> =
    Pin<Box<dyn Future<Output = Result<ToolResponse<TToolResult>>> + Send>>;
type TypedToolCallback<TToolCall, TToolResult> =
    Arc<dyn Fn(ToolRequest<TToolCall>) -> TypedToolFuture<TToolResult> + Send + Sync>;

pub struct Tool<TToolCall, TToolResult> {
    pub spec: ToolSpec,
    // Transitional metadata for provider-facing schema docs; no runtime enforcement.
    pub output_schema: Option<Value>,
    callback: TypedToolCallback<TToolCall, TToolResult>,
}

impl Tool<BorgToolCall, BorgToolResult> {
    pub fn new<F, Fut>(spec: ToolSpec, output_schema: Option<Value>, callback: F) -> Self
    where
        F: Fn(ToolRequest<Value>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ToolResponse<Value>>> + Send + 'static,
    {
        let callback = Arc::new(callback);
        Self::new_typed(spec, output_schema, move |request| {
            let callback = Arc::clone(&callback);
            async move {
                let arguments = request.arguments.to_value()?;
                let response = callback(ToolRequest {
                    tool_call_id: request.tool_call_id,
                    tool_name: request.tool_name,
                    arguments,
                })
                .await?;
                Ok(ToolResponse {
                    content: match response.content {
                        ToolResultData::Text(text) => ToolResultData::Text(text),
                        ToolResultData::Capabilities(items) => ToolResultData::Capabilities(items),
                        ToolResultData::Execution { result, duration } => {
                            ToolResultData::Execution {
                                result: BorgToolResult::from(result),
                                duration,
                            }
                        }
                        ToolResultData::Error { message } => ToolResultData::Error { message },
                    },
                })
            }
        })
    }

    pub fn new_transcoded<TCall, TResp, F, Fut>(
        spec: ToolSpec,
        output_schema: Option<Value>,
        callback: F,
    ) -> Self
    where
        TCall: DeserializeOwned + Send + 'static,
        TResp: Serialize + Send + 'static,
        F: Fn(ToolRequest<TCall>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ToolResponse<TResp>>> + Send + 'static,
    {
        let callback = Arc::new(callback);
        Self::new_typed(spec, output_schema, move |request| {
            let callback = Arc::clone(&callback);
            async move {
                let value = request.arguments.to_value()?;
                let arguments: TCall = serde_json::from_value(value)?;
                let response = callback(ToolRequest {
                    tool_call_id: request.tool_call_id,
                    tool_name: request.tool_name,
                    arguments,
                })
                .await?;
                Ok(ToolResponse {
                    content: match response.content {
                        ToolResultData::Text(text) => ToolResultData::Text(text),
                        ToolResultData::Capabilities(items) => ToolResultData::Capabilities(items),
                        ToolResultData::Execution { result, duration } => {
                            ToolResultData::Execution {
                                result: BorgToolResult::from(serde_json::to_value(result)?),
                                duration,
                            }
                        }
                        ToolResultData::Error { message } => ToolResultData::Error { message },
                    },
                })
            }
        })
    }
}

impl<TToolCall, TToolResult> Tool<TToolCall, TToolResult> {
    pub fn new_typed<F, Fut>(spec: ToolSpec, output_schema: Option<Value>, callback: F) -> Self
    where
        F: Fn(ToolRequest<TToolCall>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ToolResponse<TToolResult>>> + Send + 'static,
    {
        Self {
            spec,
            output_schema,
            callback: Arc::new(move |request| Box::pin(callback(request))),
        }
    }
}

pub struct Toolchain<TToolCall, TToolResult> {
    tools: HashMap<String, Tool<TToolCall, TToolResult>>,
}

impl<TToolCall, TToolResult> Toolchain<TToolCall, TToolResult> {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn builder() -> ToolchainBuilder<TToolCall, TToolResult> {
        ToolchainBuilder::new()
    }

    pub fn register(&mut self, tool: Tool<TToolCall, TToolResult>) -> Result<()> {
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

    pub fn merge(mut self, other: Toolchain<TToolCall, TToolResult>) -> Result<Self> {
        for (name, tool) in other.tools {
            if self.tools.contains_key(&name) {
                return Err(anyhow!("tool already registered: {}", name));
            }
            self.tools.insert(name, tool);
        }
        Ok(self)
    }
}

impl<TToolCall, TToolResult> Default for Toolchain<TToolCall, TToolResult> {
    fn default() -> Self {
        Self::new()
    }
}

impl<TToolCall, TToolResult> Toolchain<TToolCall, TToolResult> {
    pub async fn run(&self, request: ToolRequest<TToolCall>) -> Result<ToolResponse<TToolResult>> {
        let Some(tool) = self.tools.get(&request.tool_name) else {
            return Err(anyhow!("unknown tool {}", request.tool_name));
        };
        (tool.callback)(request).await
    }
}

pub struct ToolchainBuilder<TToolCall, TToolResult> {
    toolchain: Toolchain<TToolCall, TToolResult>,
}

pub type BorgToolchain = Toolchain<BorgToolCall, BorgToolResult>;

impl<TToolCall, TToolResult> Default for ToolchainBuilder<TToolCall, TToolResult> {
    fn default() -> Self {
        Self::new()
    }
}

impl<TToolCall, TToolResult> ToolchainBuilder<TToolCall, TToolResult> {
    pub fn new() -> Self {
        Self {
            toolchain: Toolchain::new(),
        }
    }

    pub fn add_tool(mut self, tool: Tool<TToolCall, TToolResult>) -> Result<Self> {
        self.toolchain.register(tool)?;
        Ok(self)
    }

    pub fn build(self) -> Result<Toolchain<TToolCall, TToolResult>> {
        Ok(self.toolchain)
    }
}
