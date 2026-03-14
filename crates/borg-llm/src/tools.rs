use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;

use crate::error::{Error, LlmResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawToolDefinition {
    pub r#type: String,
    pub function: RawToolFunction,
}

impl RawToolDefinition {
    pub fn function(
        name: impl Into<String>,
        description: Option<&str>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            r#type: "function".to_string(),
            function: RawToolFunction {
                name: name.into(),
                description: description.map(str::to_string),
                parameters,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawToolFunction {
    pub name: String,
    pub description: Option<String>,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

pub trait TypedTool: Sized + DeserializeOwned + schemars::JsonSchema + 'static {
    fn tool_definitions() -> Vec<RawToolDefinition>;

    fn decode_tool_call(name: &str, arguments: serde_json::Value) -> LlmResult<Self>;
}

impl TypedTool for () {
    fn tool_definitions() -> Vec<RawToolDefinition> {
        Vec::new()
    }

    fn decode_tool_call(name: &str, _arguments: serde_json::Value) -> LlmResult<Self> {
        Err(Error::InvalidResponse {
            reason: format!("unexpected tool call for empty tool set: {name}"),
        })
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ToolCall<C> {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub tool: C,
}

impl<C: fmt::Debug> fmt::Debug for ToolCall<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolCall")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("arguments", &self.arguments)
            .field("tool", &self.tool)
            .finish()
    }
}

#[derive(Clone)]
pub struct TypedToolSet<C> {
    _phantom: PhantomData<C>,
}

impl<C: TypedTool> TypedToolSet<C> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }

    pub fn to_tool_definitions(&self) -> Vec<RawToolDefinition> {
        C::tool_definitions()
    }
}

impl<C: TypedTool> Default for TypedToolSet<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> fmt::Debug for TypedToolSet<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TypedToolSet").finish()
    }
}
