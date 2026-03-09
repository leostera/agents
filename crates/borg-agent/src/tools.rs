use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use borg_llm::ToolDescriptor;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest<TToolCall> {
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: TToolCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse<TToolResult> {
    pub output: ToolOutputEnvelope<TToolResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum ToolOutputEnvelope<TToolResult> {
    Ok(TToolResult),
    ByDesign(TToolResult),
    Error(String),
}

pub type ToolResultData<TToolResult> = ToolOutputEnvelope<TToolResult>;

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
                    output: match response.output {
                        ToolOutputEnvelope::Ok(result) => {
                            ToolOutputEnvelope::Ok(BorgToolResult::from(result))
                        }
                        ToolOutputEnvelope::ByDesign(result) => {
                            ToolOutputEnvelope::ByDesign(BorgToolResult::from(result))
                        }
                        ToolOutputEnvelope::Error(message) => ToolOutputEnvelope::Error(message),
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
                    output: match response.output {
                        ToolOutputEnvelope::Ok(result) => ToolOutputEnvelope::Ok(
                            BorgToolResult::from(serde_json::to_value(result)?),
                        ),
                        ToolOutputEnvelope::ByDesign(result) => ToolOutputEnvelope::ByDesign(
                            BorgToolResult::from(serde_json::to_value(result)?),
                        ),
                        ToolOutputEnvelope::Error(message) => ToolOutputEnvelope::Error(message),
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
